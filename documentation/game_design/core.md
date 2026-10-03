# Essences 
They represent a characteristic that has certain pull on object identity. Like a quantified representation of a concept.

| Essence  | Affliction      |
| -------- | --------------- |
| Fire     | Burn            |
| Water    | Wet             |
| Light    | Radiant         |
| Electric | Charged         |
| Holy     | Sanctified      |
| Smoke    | Obscured        |
| Steam    | Pressurized     |
| Storm    | Gathering Storm |
| Life     | Growth          |
Essences are the building material of wisps, hence one way of collecting them is to eliminate wisps.

General design principles for essences:
- Effect essences should at least on paper remain a toolbox of the player. No overdrive should strictly punish player without a hatch or scenario where it can be leverage by the player (example: triggering transformation may be the only way to farm certain essence on map that naturally do not spawn wisps of given essence.)
- Wild combos and interactions are welcome. We aim for the fun over balance. Whatever turns out to be too broken, we will adjust after testing. 

### Afflictions
They happen when the level of alien essence invades the target and begins disrupting target object. The disruption type is usually generic, but there may be defined dedicated override for certain pairs. They also act as conductors and enablers for other effects. 

If there are more than one invading essence, they compete for who crosses the overdrive threshold.

Object innately made of given essence do not get their corresponding status effects. Ie, water is not wet(Holy is expectation - it is sanctified). Instead, the same essence heals it: essence matching the innate one restores the target's essence level.

Getting in contact with essence(for example getting shot with infused ammunition) causes invading essences to accumulate but innate essence is good at protecting itself and it is quickly cleansed. Only after Affliction is applied the invading essence gets a real foothold. That means triggering affliction via small doses is not impossible, but it is unlikely. 

#### Types:
- **Burn** - Continuous fire damage. Consumes target's innate essence to sustain itself. 
- **Wet** - Just a status effect, but enables lots of combo results.
- **Radiant** - More likely to be targeted by who ever already perceives it as enemy. 
- **Charged** - Colliding with other charged object can create spark. As it elevates towards overdrive it stuns the target for N seconds across predefined thresholds. Propagates through **Wet**
- **Sanctified** - Cleanse and prevent all other afflictions; immune to overdrive.
- **Pressurized** - each hit it takes knocks it back along it's movement path, reducing the affliction level
- **Obscured** - Hostile targeting picks it less often, may miss its attacks.
- **Gathering Storm** - Just a status effect; does not have custom overdrive effects; On overdrive turn into storm type. If there are colliding Storm types(any types), combine into single bigger and stronger **Storm Wisp** 
- **Growth** - Consumes and stores target essence. On overdrive, if no custom overdrive is provided: if the target still has more than half of its own essence, spawns a clone. They both reset their Growth and essence level. (ie, to trigger, the target must have gained essence from external sources)

Afflictions without an essence source follow the same logic but have no overdrive:
- **Slow** - Reduces movement speed.
- **Brittle** - Increases damage taken (possibly also lowers the overdrive threshold - TBD).
- **Daze** - The target loses its pathing: it keeps its current motion direction with a random skew and bounces off walls, causing partial crowding and confusion. A target already attacking keeps attacking; a moving target may walk into its target, which counts the same as bumping into a wall. When Daze ends, the target re-paths and retargets from wherever it is.

When Affliction overwhelms the innate essence, target either is destroyed or transforms.

#### Overdrives:
Fire:
- + Wet -> %X to transform into Smoke form
- + Radiant -> invulnerable for N seconds; then is destroyed. 

Water:
- + Burn -> transform into Steam form
- + Radiant -> %X to transform into Holy form
- + Charged -> Electrolysis; splits into swarm of smaller objects. Only for objects with `Splittable` trait.

Light:
- + Wet -> Mirage; is destroyed but turns into N smaller objects. They move in random directions if they have movement capability before fading. Copies don't grant essence.  

Electric:
- + Wet ->  Discharge; is destroyed, triggering nearby Charged.
- + Radiant -> is destroyed; Smite nearest **enemy** within N cells. If no valid targets, repeat search once - for **any** valid target this time.  

Smoke:
- + Charged -> %X to transform into storm form

### Interactions

**Radiant & Obscured** - while both are active they cancel each other out. Still race between each other to overdrive.

# World Phenomena
More complex and unique elements that happen in the world.

- **Mist** - Water essence pressure carried in the air. Drifts and fades over time. Cool blue, low and slowly sinking. Needs to be distinctive from steam. Sources: Humidifier, Drizzle Gun, Water Hose, Fire & Water contact.
- **Fog** - Forms where enough Mist gathers. More durable than Mist: once formed, ongoing sources sustain it far more easily than forming it took. Without sources feeding it, it dissipates. Effect TBD; it must not duplicate **Obscured** and should lean on Water's role as an effect conductor.
- **Crater** -A hollow in the ground that holds whatever fills it. Sources: Cannon impacts (filled with `Tier 2` by the base hooks), Water Hose (fills craters it hits with water). Lifecycle and how contents of different essences mix TBD.
- **Water Wall** - A blob of water plugging a gap between two walls. Masquerades as a wall but is not one and does not affect pathing; wisps pass through it under heavy slow. Source: Deluge. Whether it counts as a wall for any mechanic is TBD.

**Fire & Water** - Fire essence meeting water in the world (Mist, water-filled Craters, Water Walls) produces **Mist**. Steam never forms from such contact; it requires a full **Burn** affliction brewing into an overdrive.

# Towers 

Player, depending on scenario, starts with 5 base tower types. Each has its own distinct identity.
Towers have 2 upgrade lanes:
**Stat shard sockets** - a predefined stat shard slots, per tower type. They grant basic stats like range, attack speed, etc. Accepts any shard tier. 
**Essence shard sockets** - essence shards change a tower in two ways:

**Augmentation** - the tower stays the same type; a socketed `Tier 1` or `Tier 2` shard modifies how it works. Each tower defines its own augmentation slots: how many (possibly none), what each one does, and which essences it accepts. There is no common shape every tower must follow and no slot has to support every essence. **Hooks** are the implementation detail of an augmentation: the points in the tower's mechanic where the socketed essence takes effect.

**Evolution** - a `Tier 3` shard changes the tower into a different type that does something else (e.g. Blaster + Electric → Railgun). The new type brings its own augmentation slots. On evolving, socketed augmentation shards carry over if they fit the new slots; those that don't are returned to the player. An evolved tower can evolve further, so evolutions form a tree per base tower (e.g. Railgun → Overcharged Railgun).


## Tower Blaster
Fast, single target, blaster bullets that aim at general target direction. If target is not hit the bullet flies until it hits something or leaves the map. 

**Stat shard sockets**: Attack Speed, Range, Strength
**Base hooks**: On blaster projectile, every shot, dealing given essence damage. If `Tier 2` and `Tier 1` are present, the projectile starts Afflicted with `Tier 1` instead of dealing its damage.  
**Evolutions**:
- **Electric → Railgun** - The round is accelerated to extreme speed and pierces everything along its path until it leaves the map. `Tier 2` applies on hit. Spray `Tier 1` around. `Tier 1` afflicts `Tier 2` like in base version - having more time to mature. Spraying reduces Affliction level.
	- **Overcharged Railgun** - Railgun + a second `Tier 3` Electric shard (2x `Tier 3` Electric in total), requires special research(name TBD) - whenever bullet hits enemy, the spark links all nearby bullets, spreading outwards. 
- **Fire → Flamethrower** - The fire rate turns into a continuous stream: sprays a cone of fire in the general target direction, hitting everything inside it. Trades the bullet's unlimited flight for short range. Hooks TBD.
- **Water → Drizzle Gun** - Fires fast droplets that deal little water damage and mostly deliver essence infusions. On hit, a droplet bursts into a small puff of **Mist** that quickly fades (mini AoE). A steady stream into one area builds up Mist toward **Fog**. Hooks TBD.
- **Light → Laser** - Shots travel at the speed of light: each is an instant beam from the tower to the target, with no travel time and no misses. Hooks TBD.
## Tower Cannon
Slow, massive lobbing projectile that deals massive AoE damage. 

**Stat shard sockets**: Attack Speed, Range, Strength
**Base hooks**: An impact crater, `Tier 1` tiny hooks splashing around the centre, `Tier 2` fills the crater. Expires after N seconds. 
**Evolutions**:
- **Electric → Sky Hammer** - Summons a thunderbolt within range, telegraphed by a small circle that shrinks until the strike lands. The bolt is drawn to **Charged** objects inside the circle. Hooks same as base. Adds a Projectile Count stat shard socket.
- **Fire → Blaze Mortar** - Lobs fireballs that hit hard over a bigger area: a 2x2 main blast surrounded by a 4x4 secondary blast. Hooks TBD.
- **Water → Deluge** - Lobs a huge ball of water splashing a 3x3 area. Creates no crater. The center takes water damage; the outer ring gets essence infusion with brief displacement and slow, as in other water effects. If the center lands in a cell with walls on both opposing sides (either axis; only walls count), the water fits the gap like a blob and forms a temporary **Water Wall**. Mostly a tool for effects: the Water Wall is a potent spot for creating **Mist**. Hooks TBD.
- **Light → Starshell** - Throws a flare into the air on a not-too-fast lob, applying a bit of light essence around it. In the air it bursts, casting a targeted cone of light from the air down to the ground in its general flight direction; every wisp inside the cone's footprint gets **Daze**. Hooks TBD.

## Tower Rocket Launcher
Medium reload speed and medium damage to target + reduced AoE damage. Massive range. Launches guided projectile that switches target if current target is destroyed. Has a minimum range limitation. 

**Stat shard sockets**: Attack Speed, Range, Projectile Speed
**Base hooks**: Hit moment: Apply `Tier 2` affliction to direct target. Shower surroundings with `Tier 1` essence damage. 
**Evolutions**:
- **Electric → Lightning Spire** - Launches a slow orb of ball lightning that hunts its target, zapping everything it passes. Has its own lifetime, draining more on zaps and picks new random target within tower range on target destruction. `Tier 2` passive short aura of given essence expanding from the orb. `Tier 1` %X chance to flavor the zap. 
- **Fire → Sky Lanterns** - Releases lanterns that drift slowly across the map in a direction the player sets (within a limited angle), exuding a heat aura that applies fire essence pressure to wisps below. They target no one and fly until they leave the map, so wisps can be pre-heated from the far edge of the map.
	- **Drift** - lanterns adjust their heading here and there as they fly. Nearby wisps give a lantern a small boost toward them, but only when the wisp lies within an acceptable match of the lantern's own direction.
	- **Altitude** - lanterns rise slowly after release and the aura activates only once they reach altitude, leaving the area around the tower unaffected.
	- Hooks TBD.
- **Water → Water Hose** - Shoots a lobbed stream of water at a far-away target; only the target area is hit. Deals basic water damage, infuses water essence and fills any **Crater** it hits. The stream slowly drags after a moving target during ejection, so fast wisps can outrun it. Leaves small **Mist** pockets along the lob, which sink onto whatever lies beneath. Hooks TBD.
- **Light → Mirror Array** - Fires a laser up to a mirror drone in the air, which bounces it down onto a single grid cell, frying everything that passes through it when it fires. Massive damage at the cost of manual setup, making it strongest at chokepoints.
	- **Drones** - bought separately and reuse the existing drone logic: they fly to their position and have to return to refuel after some time, coming back automatically afterwards. They fly in the air, out of reach of normal wisps; ways they can be endangered are TBD.
	- **Targeting** - the player selects the grid cell to target and the drone positions itself so the bounced laser hits it. One cell per drone, a single bounce only. The tower may have more than one drone slot, aiming at several cells at once.
	- Hooks TBD.

## Tower Emitter
Emits waves spreading in all directions. Deals no damage; applies Brittle effect. When modified beyond base level its main identity is to provide disruptive shockwaves capable injecting enough essence in one shot to instantly turn it into affliction.

**Stat shard sockets**: Attack Speed, Range, Projectile Speed
**Base hooks**: The wave front(`Tier 2`), and fall off (`Tier 1`). Both adding only corresponding essence damage. 
**Evolutions**:
- **Electric → Synapse Coil** - Each pulse throws out tendrils (plasma globe) instead of a uniform wave. Every tendril bends toward targets within a forward cone; its attraction strength is randomized per tendril, so some swing in hard while others wander past. A lone target typically draws a few tendrils, a cluster gets hit by chains of forks. Adds a Projectile Count stat shard socket.
	1. **Seek** - the tendril grows outward, bending toward a target.
	2. **Connect** - it touches the target.
	3. **Pulse** - a single essence-colored pulse travels along the connection and delivers the whole `Tier 2` dose at once.
	4. **Fork** - the arc splits onward toward the next targets, then fades. Forks carry `Tier 1`, weaker with each fork generation.
	- `Tier 3` (Electric) is the tendrils' base damage. `Tier 2` is the afflictor, delivered by the pulse. `Tier 1` dusts the spread through forks.
	- Only hits create real forks. Without targets, tendrils run out to range as wandering spokes with short cosmetic twigs that cannot hit; each pulse starts at a random angle. 
- **Fire → Inferno** - Hoards heat and releases it all at once, instantly applying **Burn** to every wisp in range. The flash deals no additional damage; Burn does the work over time.
	- **Charging** - heat haze shimmers inward from the edge of range and the core glows from dull red to bright orange, telling the player how close the release is.
	- **Flash** - a dome of deep orange-red fire covering the whole range appears at full size in a single frame; it never grows from the centre, so nothing reads as travelling. Every wisp inside flares at the same moment. The dome breaks up in place into falling embers and fading heat haze, and the tower drops back to dark, cooling metal.
	- **Feeding** - draws heat out of nearby Fire wisps to charge faster, draining their essence. The flash then heals them, since same essence restores the innate one.
	- Hooks TBD.
- **Water → Tide Bell** - Each pulse is a real water wave rolling outward. It displaces wisps as it passes, briefly slows them, and instantly applies **Wet**. Hooks TBD.
- **Light → Lightbearer** - Gives light to the wisps inside. The tower switches on like a lamp, filling its range with diffused light, then switches off - a single on/off, never a flicker. While lit, it locks onto every wisp inside with an additional lock-on visual; after a moment the light fades and all locked targets become **Radiant**. Wisps that leave the light before the lock completes escape, so keeping wisps in range pays off. Hooks TBD.

## Tower Field
Constant field applying slow effect. When modified beyond base level its main identity is to provide constant essence pressure to speed up Overdrives. 

**Stat shard sockets**: Range, Strength 
**Base hooks**: The boundary, `Tier 2`, spike essence injection into target. `Tier 1`, in range, constant slow injection.
**Evolutions**:
- **Electric → Circuit Lattice** - Ground tiles in range turn into a lattice board of glowing circuit traces. Current pulses along the traces; objects on a trace when a pulse passes are hit. Pulses crossing at a junction flare. `Tier 2` on junction flares, `Tier 1` as constant hum along the traces. Its steady injection pairs with **Charged** stuns across the path.
- **Fire → Crucible** - A dome of heat that deals no direct damage; everything inside takes fire essence impact.
	- **Enemies** - constant fire essence pressure, building toward **Burn**.
	- **Towers** - a tower with an empty `Tier 1` slot gains an ephemeral Fire `Tier 1` while inside; a tower with `Tier 1` already filled gains extra fire damage instead.
	- **Buildings** - every building inside heats up and overheats, periodically pausing to cool down.
	- **Projectiles** - friendly projectiles passing through the dome come out heated and carry fire to whatever they hit, so towers outside the dome can fire through it without overheating.
	- Hooks TBD.
- **Water → Humidifier** - A rotating dispenser sprays **Mist** around the tower. The mist slowly drifts away from the tower while fading. On its own it only applies water essence pressure; what the mist conducts is TBD, in line with Water's role as an effect conductor.
	- Hooks TBD.
- **Light → Lighthouse** - Shines a big spot of light on the ground that constantly moves through the tower's range, applying light essence pressure to everything it passes over. The spot follows a mechanical sweep, giving the tower a readable rhythm. Other sweep patterns may come from world rule alterations found on the map and/or research. Hooks TBD.


Ideas for future:
- Light & Darkness - Moth Lamp - charms wisps for a while to stay near the lamp 