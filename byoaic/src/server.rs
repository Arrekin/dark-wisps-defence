use axum::{http::StatusCode, response::{IntoResponse, Response}};
use bevy::{
    ecs::{system::command::run_system_cached_with, world::CommandQueue},
    prelude::*,
};

use logging::prelude::*;

use crate::{
    collection::{ResponseCollection, collect},
    endpoints,
    ingress::ByoaicIngress,
    requests::ResponseSender,
};

const SERVER_ADDRESS: &str = "127.0.0.1:7461";

/// The game has stopped draining the ingress channel. Answers HTTP 503.
pub(crate) struct IngressClosed;
impl IntoResponse for IngressClosed {
    fn into_response(self) -> Response {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// Router state shared by all endpoint handlers.
#[derive(Clone)]
pub(crate) struct ServerState {
    ingress: async_channel::Sender<CommandQueue>,
}
impl ServerState {
    /// Hands `command` to the game; it runs with full world access at the next `PreUpdate`.
    /// Errors returned by the command go to the world's fallback error handler, as with
    /// `Commands::queue`.
    pub(crate) fn submit(&self, command: impl Command) -> Result<(), IngressClosed> {
        let mut queue = CommandQueue::default();
        queue.push(command.handle_error());
        self.ingress.try_send(queue).map_err(|_| IngressClosed)
    }

    /// Runs `system` as a cached one-shot system at the next `PreUpdate` with a fresh response
    /// sender and `input`, then collects the reports until the request ends. The system typically
    /// hands the sender to `spawn_request`. A system that cannot run drops the sender, which ends
    /// the collection as `Interrupted`.
    pub(crate) async fn request<I, M>(
        &self,
        system: impl IntoSystem<In<(ResponseSender, I)>, (), M> + Send + 'static,
        input: I,
    ) -> Result<ResponseCollection, IngressClosed>
    where
        I: Send + 'static,
        M: 'static,
    {
        let (sender, receiver) = async_channel::unbounded();
        self.submit(run_system_cached_with(system, (sender, input)))?;
        Ok(collect(receiver).await)
    }

    /// Runs `system` as a cached one-shot system at the next `PreUpdate` and returns its output.
    /// A system that cannot run drops the answer and yields `IngressClosed`; its error goes to the
    /// fallback error handler.
    pub(crate) async fn query<T, M>(&self, system: impl IntoSystem<(), T, M> + Send + 'static) -> Result<T, IngressClosed>
    where
        T: Send + 'static,
        M: 'static,
    {
        let (sender, receiver) = async_channel::bounded(1);
        self.submit(move |world: &mut World| -> Result {
            let value = world.run_system_cached(system)?;
            let _ = sender.try_send(value);
            Ok(())
        })?;
        receiver.recv().await.map_err(|_| IngressClosed)
    }
}

#[log_tags(Tag::Byoaic)]
pub(crate) fn start_server(mut commands: Commands) {
    let (sender, receiver) = async_channel::unbounded();
    commands.insert_resource(ByoaicIngress { receiver });
    let state = ServerState { ingress: sender };
    let _ = std::thread::Builder::new()
        .name("byoaic-server".into())
        .spawn(move || run_server(state))
        .inspect_err(|error| error_dev!("Failed to spawn server thread: {error}"));
}

/// Server thread body. Returns (leaving the game running without the server) when the runtime or
/// the listener cannot be created.
#[log_tags(Tag::Byoaic)]
fn run_server(state: ServerState) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        #[error_dev("Failed to build server runtime: {error}")]
        Err(error) => return,
    };
    runtime.block_on(async move {
        #[info_dev("Listening on http://{SERVER_ADDRESS}")]
        let listener = match tokio::net::TcpListener::bind(SERVER_ADDRESS).await {
            Ok(listener) => listener,
            #[error_dev("Failed to bind {SERVER_ADDRESS}: {error}")]
            Err(error) => return,
        };
        let _ = axum::serve(listener, endpoints::router(state)).await
            .inspect_err(|error| error_dev!("Server stopped: {error}"));
    });
}
