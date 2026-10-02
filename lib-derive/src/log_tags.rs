//! # `#[log_tags]` expansion
//!
//! Walks a function body, lifts log annotations off statements and match arms, and splices
//! `logging::Log` calls into the matching branch. The public contract and the expansion
//! cheatsheet live on the `log_tags` attribute in the crate root.
//!
//! Proc macros see syntax, not types, so every decision here is made from the statement's
//! shape. Where the shape does not say which branch a log belongs to, expansion fails with
//! an error pointing at the annotation instead of guessing.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::Parser, parse_quote, punctuated::Punctuated, spanned::Spanned, visit_mut::VisitMut,
    Arm, Attribute, Block, Expr, Ident, ItemFn, Meta, Path, Stmt, Token,
};

const LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];
const AUDIENCES: [&str; 2] = ["dev", "player"];

pub(crate) fn expand(attribute: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let tags = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(attribute)?;
    let mut function: ItemFn = syn::parse2(item)?;

    let tags_call = if tags.is_empty() {
        quote!()
    } else {
        let tags = tags.iter();
        quote!(.tags([#(#tags),*]))
    };
    let mut expander = Expander { tags_call, errors: Vec::new() };
    expander.visit_block_mut(&mut function.block);

    let errors = expander.errors.iter().map(syn::Error::to_compile_error);
    Ok(quote! {
        #function
        #(#errors)*
    })
}

/// One log annotation, lifted off the statement or arm it was attached to.
struct LogSite {
    span: Span,
    level: Ident,
    audience: Ident,
    message_arguments: TokenStream2,
}

impl LogSite {
    /// Recognizes `<level>_<audience>` names; anything else is not a log annotation.
    fn from_path(path: &Path, message_arguments: TokenStream2) -> Option<Self> {
        let name = path.get_ident()?.to_string();
        let (level, audience) = name.split_once('_')?;
        if !LEVELS.contains(&level) || !AUDIENCES.contains(&audience) {
            return None;
        }
        Some(Self {
            span: path.span(),
            level: Ident::new(level, path.span()),
            audience: Ident::new(audience, path.span()),
            message_arguments,
        })
    }
}

struct Expander {
    tags_call: TokenStream2,
    errors: Vec<syn::Error>,
}

impl Expander {
    fn log_statement(&self, site: &LogSite) -> Stmt {
        let LogSite { level, audience, message_arguments, .. } = site;
        let tags_call = &self.tags_call;
        parse_quote! {
            ::logging::Log::#level().#audience() #tags_call .message(::std::format_args!(#message_arguments));
        }
    }

    /// Removes the log annotation from `attributes`, if they carry one.
    fn take_log_attribute(&mut self, attributes: &mut Vec<Attribute>) -> Option<LogSite> {
        let mut sites = Vec::new();
        attributes.retain(|attribute| match self.parse_log_attribute(attribute) {
            Some(site) => {
                sites.push(site);
                false
            }
            None => true,
        });
        if let Some(extra) = sites.get(1) {
            self.errors.push(syn::Error::new(extra.span, "only one log annotation is allowed here"));
        }
        sites.into_iter().next()
    }

    fn parse_log_attribute(&mut self, attribute: &Attribute) -> Option<LogSite> {
        let (path, message_arguments) = match &attribute.meta {
            Meta::List(list) => (&list.path, list.tokens.clone()),
            meta => (meta.path(), TokenStream2::new()),
        };
        let mut site = LogSite::from_path(path, message_arguments)?;
        if site.message_arguments.is_empty() {
            self.errors.push(syn::Error::new(attribute.span(), "log annotations take a message: `#[info_dev(\"...\")]`"));
            site.message_arguments = quote!("");
        }
        Some(site)
    }

    fn take_statement_log_attribute(&mut self, statement: &mut Stmt) -> Option<LogSite> {
        let attributes = match statement {
            Stmt::Local(local) => &mut local.attrs,
            Stmt::Macro(statement_macro) => &mut statement_macro.attrs,
            Stmt::Expr(expression, _) => return self.take_expression_log_attribute(expression),
            Stmt::Item(_) => return None,
        };
        self.take_log_attribute(attributes)
    }

    /// syn attaches an expression statement's outer attributes to the outermost expression
    /// for postfix chains (`a.b()`, `a?`) but to the leftmost operand for binary-like ones
    /// (`#[x] a + b` puts `#[x]` on `a`), so the annotation is searched down the left spine.
    fn take_expression_log_attribute(&mut self, expression: &mut Expr) -> Option<LogSite> {
        let mut current = expression;
        loop {
            if let Some(attributes) = expression_attributes_mut(current)
                && let Some(site) = self.take_log_attribute(attributes)
            {
                return Some(site);
            }
            current = match current {
                Expr::Assign(inner) => &mut inner.left,
                Expr::Binary(inner) => &mut inner.left,
                Expr::Cast(inner) => &mut inner.expr,
                Expr::Range(inner) => inner.start.as_mut()?,
                _ => return None,
            };
        }
    }

    /// Expands a `info_dev!(...)`-style statement macro into its log statement.
    fn expand_statement_macro(&self, statement: &Stmt) -> Option<Stmt> {
        let Stmt::Macro(statement_macro) = statement else { return None };
        let macro_call = &statement_macro.mac;
        let site = LogSite::from_path(&macro_call.path, macro_call.tokens.clone())?;
        Some(self.log_statement(&site))
    }

    /// Logs at the end of a branch, before its exit.
    fn log_in_branch(&self, site: &LogSite, branch: &mut Expr) {
        if !matches!(branch, Expr::Block(_)) {
            *branch = parse_quote!({ #branch });
        }
        if let Expr::Block(branch_block) = branch {
            self.log_in_block(site, &mut branch_block.block);
        }
    }

    /// Logs at the end of a block, before its exit.
    fn log_in_block(&self, site: &LogSite, block: &mut Block) {
        let log = self.log_statement(site);
        match block.stmts.pop() {
            Some(last) => place_log_around(last, log, true, &mut block.stmts),
            None => block.stmts.push(log),
        }
    }

    /// Places the log for an annotated statement: into its branch if it has one,
    /// otherwise around the statement itself.
    fn attach(&mut self, site: LogSite, mut statement: Stmt, is_tail: bool, output: &mut Vec<Stmt>) {
        let placed_in_branch = match &mut statement {
            Stmt::Local(local) => match local.init.as_mut().and_then(|init| init.diverge.as_mut()) {
                Some((_, else_expression)) => {
                    self.log_in_branch(&site, else_expression);
                    true
                }
                None => false,
            },
            Stmt::Expr(Expr::If(if_expression), _) => {
                if if_expression.else_branch.is_some() {
                    self.errors.push(syn::Error::new(site.span, "an `if` with an `else` has more than one branch: put the log inside the branch it belongs to"));
                } else {
                    self.log_in_block(&site, &mut if_expression.then_branch);
                }
                true
            }
            Stmt::Expr(Expr::Match(_), _) => {
                self.errors.push(syn::Error::new(site.span, "a `match` has more than one branch: annotate the arm the log belongs to"));
                true
            }
            _ => false,
        };
        if placed_in_branch {
            output.push(statement);
        } else {
            place_log_around(statement, self.log_statement(&site), is_tail, output);
        }
    }
}

impl VisitMut for Expander {
    fn visit_block_mut(&mut self, block: &mut Block) {
        let statements = std::mem::take(&mut block.stmts);
        let statement_count = statements.len();
        for (index, mut statement) in statements.into_iter().enumerate() {
            let site = self.take_statement_log_attribute(&mut statement);
            match self.expand_statement_macro(&statement) {
                Some(log) => statement = log,
                None => syn::visit_mut::visit_stmt_mut(self, &mut statement),
            }
            match site {
                Some(site) => self.attach(site, statement, index + 1 == statement_count, &mut block.stmts),
                None => block.stmts.push(statement),
            }
        }
    }

    fn visit_arm_mut(&mut self, arm: &mut Arm) {
        let site = self.take_log_attribute(&mut arm.attrs);
        syn::visit_mut::visit_arm_mut(self, arm);
        if let Some(site) = site {
            self.log_in_branch(&site, &mut arm.body);
        }
    }

    /// Expands `info_dev!(...)`-style macros in expression position, e.g. a match arm body.
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Macro(expression_macro) = expression
            && let Some(site) = LogSite::from_path(&expression_macro.mac.path, expression_macro.mac.tokens.clone())
        {
            let log = self.log_statement(&site);
            *expression = parse_quote!({ #log });
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }
}

/// Pushes `statement` and `log` onto `output` so the log runs once the statement has run:
/// - an exit (`return`/`continue`/`break`) keeps its place last, the log goes before it; a
///   value it carries is evaluated first, so a failing `?` or a panic in it skips the log;
/// - a tail value is evaluated first, then logged, then yielded;
/// - any other statement is followed by the log.
fn place_log_around(mut statement: Stmt, log: Stmt, is_tail: bool, output: &mut Vec<Stmt>) {
    match &mut statement {
        Stmt::Expr(Expr::Return(syn::ExprReturn { expr: exit_value, .. }) | Expr::Break(syn::ExprBreak { expr: exit_value, .. }), _) => {
            if let Some(expression) = exit_value.take() {
                let value = value_binding();
                output.push(parse_quote! {
                    #[allow(clippy::let_unit_value)]
                    let #value = #expression;
                });
                *exit_value = Some(parse_quote!(#value));
            }
            output.push(log);
            output.push(statement);
        }
        Stmt::Expr(Expr::Continue(_), _) => {
            output.push(log);
            output.push(statement);
        }
        Stmt::Expr(tail, None) if is_tail => {
            let value = value_binding();
            output.push(parse_quote! {
                #[allow(clippy::let_unit_value)]
                let #value = #tail;
            });
            output.push(log);
            output.push(Stmt::Expr(parse_quote!(#value), None));
        }
        _ => {
            output.push(statement);
            output.push(log);
        }
    }
}

/// The variable holding a value computed before its log. Mixed-site hygiene keeps it from
/// clashing with, or being visible to, the user's own code.
fn value_binding() -> Ident {
    Ident::new("value", Span::mixed_site())
}

/// Outer attributes of an expression statement. syn attaches them to the outermost expression.
fn expression_attributes_mut(expression: &mut Expr) -> Option<&mut Vec<Attribute>> {
    Some(match expression {
        Expr::Array(inner) => &mut inner.attrs,
        Expr::Assign(inner) => &mut inner.attrs,
        Expr::Async(inner) => &mut inner.attrs,
        Expr::Await(inner) => &mut inner.attrs,
        Expr::Binary(inner) => &mut inner.attrs,
        Expr::Block(inner) => &mut inner.attrs,
        Expr::Break(inner) => &mut inner.attrs,
        Expr::Call(inner) => &mut inner.attrs,
        Expr::Cast(inner) => &mut inner.attrs,
        Expr::Closure(inner) => &mut inner.attrs,
        Expr::Const(inner) => &mut inner.attrs,
        Expr::Continue(inner) => &mut inner.attrs,
        Expr::Field(inner) => &mut inner.attrs,
        Expr::ForLoop(inner) => &mut inner.attrs,
        Expr::Group(inner) => &mut inner.attrs,
        Expr::If(inner) => &mut inner.attrs,
        Expr::Index(inner) => &mut inner.attrs,
        Expr::Infer(inner) => &mut inner.attrs,
        Expr::Let(inner) => &mut inner.attrs,
        Expr::Lit(inner) => &mut inner.attrs,
        Expr::Loop(inner) => &mut inner.attrs,
        Expr::Macro(inner) => &mut inner.attrs,
        Expr::Match(inner) => &mut inner.attrs,
        Expr::MethodCall(inner) => &mut inner.attrs,
        Expr::Paren(inner) => &mut inner.attrs,
        Expr::Path(inner) => &mut inner.attrs,
        Expr::Range(inner) => &mut inner.attrs,
        Expr::RawAddr(inner) => &mut inner.attrs,
        Expr::Reference(inner) => &mut inner.attrs,
        Expr::Repeat(inner) => &mut inner.attrs,
        Expr::Return(inner) => &mut inner.attrs,
        Expr::Struct(inner) => &mut inner.attrs,
        Expr::Try(inner) => &mut inner.attrs,
        Expr::TryBlock(inner) => &mut inner.attrs,
        Expr::Tuple(inner) => &mut inner.attrs,
        Expr::Unary(inner) => &mut inner.attrs,
        Expr::Unsafe(inner) => &mut inner.attrs,
        Expr::While(inner) => &mut inner.attrs,
        Expr::Yield(inner) => &mut inner.attrs,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream as TokenStream2;
    use quote::{quote, ToTokens};
    use syn::ItemFn;

    use super::expand;

    fn expanded(body: TokenStream2) -> String {
        expand(quote!(Tag::Ui), quote!(fn f() #body)).unwrap().to_string()
    }

    fn assert_expands(body: TokenStream2, expected_body: TokenStream2) {
        let expected: ItemFn = syn::parse2(quote!(fn f() #expected_body)).unwrap();
        assert_eq!(expanded(body), expected.to_token_stream().to_string());
    }

    fn assert_rejected(body: TokenStream2, message: &str) {
        let output = expanded(body);
        assert!(output.contains("compile_error"), "expected a compile error in: {output}");
        assert!(output.contains(message), "expected `{message}` in: {output}");
    }

    fn warn_log() -> TokenStream2 {
        quote!(::logging::Log::warn().dev().tags([Tag::Ui]).message(::std::format_args!("m"));)
    }

    #[test]
    fn let_else_logs_before_the_exit_of_the_else_block() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] let Some(x) = a else { continue }; }),
            quote!({ let Some(x) = a else { #log continue }; }),
        );
    }

    #[test]
    fn guard_if_logs_before_its_exit() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] if !paid { return None; } }),
            quote!({ if !paid { #[allow(clippy::let_unit_value)] let value = None; #log return value; } }),
        );
    }

    #[test]
    fn action_if_logs_at_the_end_of_its_block() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] if done { let item = take(); store(item); } }),
            quote!({ if done { let item = take(); store(item); #log } }),
        );
    }

    #[test]
    fn match_arm_value_is_evaluated_before_the_log() {
        let log = warn_log();
        assert_expands(
            quote!({ match kind { #[warn_dev("m")] other => None, } }),
            quote!({ match kind { other => { #[allow(clippy::let_unit_value)] let value = None; #log value }, } }),
        );
    }

    #[test]
    fn match_arm_exit_keeps_its_place_last() {
        let log = warn_log();
        assert_expands(
            quote!({ match kind { #[warn_dev("m")] _ => continue, } }),
            quote!({ match kind { _ => { #log continue }, } }),
        );
    }

    #[test]
    fn question_mark_statement_logs_after_it_succeeds() {
        let log = warn_log();
        assert_expands(
            quote!({
                #[warn_dev("m")] let x = a?.b()?;
                #[warn_dev("m")] let y = c?.field;
                #[warn_dev("m")] d?;
            }),
            quote!({
                let x = a?.b()?; #log
                let y = c?.field; #log
                d?; #log
            }),
        );
    }

    #[test]
    fn plain_statement_logs_after_it_runs() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] let rows = collect(); consume(rows); }),
            quote!({ let rows = collect(); #log consume(rows); }),
        );
    }

    #[test]
    fn exit_without_value_logs_before_it() {
        let log = warn_log();
        assert_expands(
            quote!({ loop { #[warn_dev("m")] continue; #[warn_dev("m")] break; } #[warn_dev("m")] return; }),
            quote!({ loop { #log continue; #log break; } #log return; }),
        );
    }

    #[test]
    fn exit_value_is_evaluated_before_the_log() {
        let log = warn_log();
        assert_expands(
            quote!({
                'outer: loop { #[warn_dev("m")] break 'outer compute(); }
                #[warn_dev("m")] return something()?;
            }),
            quote!({
                'outer: loop { #[allow(clippy::let_unit_value)] let value = compute(); #log break 'outer value; }
                #[allow(clippy::let_unit_value)] let value = something()?; #log return value;
            }),
        );
    }

    #[test]
    fn exit_value_in_a_branch_is_evaluated_before_the_log() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] let Some(x) = a else { return fallback()?; }; }),
            quote!({ let Some(x) = a else { #[allow(clippy::let_unit_value)] let value = fallback()?; #log return value; }; }),
        );
    }

    #[test]
    fn tail_value_is_evaluated_before_the_log() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] compute() }),
            quote!({ #[allow(clippy::let_unit_value)] let value = compute(); #log value }),
        );
    }

    #[test]
    fn annotation_on_a_binary_expression_is_found_on_its_left_operand() {
        let log = warn_log();
        assert_expands(
            quote!({ #[warn_dev("m")] a + b }),
            quote!({ #[allow(clippy::let_unit_value)] let value = a + b; #log value }),
        );
    }

    #[test]
    fn statement_and_expression_macros_expand_in_place() {
        let log = warn_log();
        assert_expands(
            quote!({ warn_dev!("m"); match kind { _ => warn_dev!("m"), } }),
            quote!({ #log match kind { _ => { #log }, } }),
        );
    }

    #[test]
    fn unrelated_attributes_are_left_alone() {
        assert_expands(
            quote!({ #[allow(unused)] let x = 1; }),
            quote!({ #[allow(unused)] let x = 1; }),
        );
    }

    #[test]
    fn function_without_tags_logs_without_tags() {
        let output = expand(quote!(), quote!(fn f() { #[warn_dev("m")] let x = 1; })).unwrap().to_string();
        let expected: ItemFn = syn::parse2(quote!(fn f() {
            let x = 1;
            ::logging::Log::warn().dev().message(::std::format_args!("m"));
        })).unwrap();
        assert_eq!(output, expected.to_token_stream().to_string());
    }

    #[test]
    fn if_with_else_is_rejected() {
        assert_rejected(quote!({ #[warn_dev("m")] if a { b(); } else { c(); } }), "an `if` with an `else`");
    }

    #[test]
    fn whole_match_is_rejected() {
        assert_rejected(quote!({ #[warn_dev("m")] match a { _ => {} } }), "a `match` has more than one branch");
    }

    #[test]
    fn second_annotation_is_rejected() {
        assert_rejected(quote!({ #[warn_dev("m")] #[info_dev("n")] let x = 1; }), "only one log annotation");
    }

    #[test]
    fn annotation_without_message_is_rejected() {
        assert_rejected(quote!({ #[warn_dev] let x = 1; }), "log annotations take a message");
    }
}
