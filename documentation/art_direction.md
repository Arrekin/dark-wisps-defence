# Art Direction & UI Design Principles

The visual identity of the game: cold, void, ambient, mysterious, dark — but expressive,
not muted. The interface should feel like instrumentation aboard something observing an
anomaly: controlled, precise, alive.

The core bet: most UI surfaces are drawn procedurally by shaders rather than bitmap
assets. Assets are reserved for icons, fonts, and the occasional decorative element.
Everything else — panels, borders, glows, energy, gauges — is generated, which keeps the
interface resolution-independent, animatable, and cheap to iterate on.

## Palette

| Role | Hex |
|---|---|
| Abyss background | `#03040A` |
| Panel background | `#080D1A` |
| Elevated surface | `#0D1630` |
| Structural border | `#233A68` |
| Primary text | `#EAF4FF` |
| Secondary text | `#8BA8CC` |
| Ice blue | `#28C7FF` |
| Ultraviolet | `#7657FF` |
| Spectral violet | `#B45CFF` |
| Danger magenta | `#FF3D8D` |
| Requirement met | `#35B87A` |
| Rare positive state | `#42F5C8` |

Accent semantics are fixed:

* **Ice blue** — the default interactive color: hover, selection, focus.
* **Violet** — anomalous energy and exotic phenomena.
* **Magenta** — danger and irreversible actions.
* **Green** — a requirement already satisfied; the ordinary positive, as on an affordable
  cost. Quieter than magenta in both saturation and value, so a screen reads calm until
  something blocks. Its hue sits well clear of ice blue, so a met requirement is never
  mistaken for something interactive.
* **Teal** — rare positive states; used sparingly so it stays special.

Green and teal are not interchangeable. Green is the common case answering "yes"; teal marks
something uncommon enough to be worth stopping at.

Never use every accent simultaneously. A screen should read as mostly dark with one or
two accents carrying meaning.

UI accent semantics must not collide with gameplay color semantics. The HUD floats over
a playfield that already assigns meaning to colors (wisps, towers, effects). When
introducing or recoloring gameplay elements, check the combined reading — "violet =
anomalous" must hold on both sides of the glass.

Danger is never encoded in hue alone. Magenta reads weakly for some forms of color
blindness; danger states must also carry shape (iconography) and motion (the slow pulse)
so the signal survives without color.

## Depth Model

Depth comes from value differences, not transparency stacking:

* Root background: almost black.
* Panels: barely lighter.
* Raised controls: another small step lighter.
* Borders: cold blue at low opacity.
* Selected elements: saturated border plus local glow.

**Most panels do not glow.** Glow is reserved for selection and importance — if
everything emits light, nothing appears important. This is the single most important
rule in the direction.

A faint star field behind the interface can establish the void, but keep it away from
text-heavy areas.

## Panel Shader

One general void-panel material covers most surfaces, composed from inexpensive layers:

1. Nearly black vertical or radial gradient
2. Thin outer border
3. Brighter inner border at low opacity
4. Subtle illumination near selected edges
5. Sparse procedural specks or noise
6. Optional angular corner cuts
7. Animated energy traveling along the border

Variation comes from parameters, not from new shaders:

* background colors
* border color and width
* corner-cut size
* edge brightness
* noise intensity
* energy position and speed
* selected amount
* danger amount

Borders and corner cuts are signed-distance functions with derivative-based
antialiasing so they stay sharp at any resolution. Border widths and corner cuts are
specified in pixels, not UV fractions — the shader must know the surface's size so
geometry doesn't stretch with panel dimensions.

Noise and specks approach zero intensity behind dense text and data. Texture belongs on
empty panel area; readability wins everywhere else.

## Material Families

Three families, not one enormous shader and not a shader per widget:

* **Void panel** — backgrounds, borders, corner cuts, and interactive states (hover,
  selection, danger). Buttons and tabs are this material with the interactive
  parameters driven.
* **Data display** — graphs, segmented bars, radial gauges.
* **Set-piece visualization** — any focal, living visual (a centerpiece a screen is
  built around), rendered on its own surface behind the interface, decoupled from UI
  layout so it can animate freely without constraining or being constrained by panels.

## Motion Language

Cold and controlled:

* Hover: 100–160 ms edge illumination.
* Selection: a quick energy sweep, then a stable glow.
* Ambient border motion: 4–8 seconds per cycle.
* Warning: slow magenta pulse — never rapid flashing.
* Data changes: numbers snap; bars interpolate smoothly.
* Set-pieces: continuous slow motion with occasional interference.

Different systems run at different motion frequencies. If every component pulses in
sync, the screen feels artificial.

Continuous motion (ambient energy, pulses, sweeps) is computed inside shaders from
global time. Game state supplies discrete targets — selected or not, danger or not —
and the transition toward the target is eased on the shader side or by one central
animation mechanism. Per-widget ad-hoc parameter tweening is how motion languages rot.

## Typography and Icons

Shaders cannot compensate for weak typography.

* Headings: Space Grotesk
* Body: Inter
* Data: JetBrains Mono

Uppercase for short headings only; body text conventionally
spaced and readable. One consistent outline icon family throughout — Lucide is the
starting point.
