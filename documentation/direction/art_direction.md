# Art Direction & UI Design Principles

The visual identity of the game: cold, void, ambient, mysterious, dark — but expressive,
not muted.

Most UI surfaces are drawn procedurally by shaders, which keeps the
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

Accent semantics:

* **Ice blue** — the default interactive color: hover, selection, focus.
* **Violet** — anomalous energy and exotic phenomena.
* **Magenta** — danger and irreversible actions.
* **Green** — a requirement already satisfied; the ordinary positive.
* **Teal** — rare positive states; used sparingly so it stays special.

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

## Panel Shader

One general void-panel material covers most surfaces, composed from inexpensive layers:

1. Nearly black vertical or radial gradient
2. Thin outer border
3. Brighter inner border at low opacity
4. Subtle illumination near selected edges
5. Sparse procedural specks or noise
6. Optional angular corner cuts
7. Animated energy traveling along the border

Variations:

* background colors
* border color and width
* corner-cut size
* edge brightness
* noise intensity
* energy position and speed

## Material Families

* **Void panel** — backgrounds, borders, corner cuts, and interactive states (hover,
  selection, danger). Buttons and tabs are this material with the interactive
  parameters driven.
* **Data display** — graphs, segmented bars, radial gauges.

## Motion Language

* Hover: 100–160 ms edge illumination.
* Selection: a quick energy sweep, then a stable glow.
* Ambient border motion: 4–8 seconds per cycle.
* Warning: slow magenta pulse — never rapid flashing.
* Data changes: numbers snap; bars interpolate smoothly.
* Set-pieces: continuous slow motion with occasional interference.

Different systems run at different motion frequencies. If every component pulses in
sync, the screen feels artificial.

## Typography

* Headings: Space Grotesk
* Body: Inter
* Data: JetBrains Mono
