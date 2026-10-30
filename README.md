# 3D World Movie Studio

**Describe a world. Cast it. Direct it in words. Get a film.**

An open-source, world-first AI movie studio from [Thinkelution](https://github.com/Thinkelution).
Instead of generating video clip by clip, it builds a persistent 3D world with places,
time of day, weather and characters with their own faces and voices. It then performs
and films that world, with camera shots you describe in ordinary film language.

Everything domain-specific is our own: engine, renderer, world builder, character
generator, rigging, animation, camera system and script language. No Blender, Unreal
or Unity.

> **Status: pre-alpha, planning.** There's no code yet. Development is being built in public.

## Read first

- [VISION.md](VISION.md): what we're building and why
- [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md): architecture, milestones and the road to the first scene

## A taste of the script

```text
PLACE  tea-stall: a roadside tea stall with a tin awning, wet street, a bus stop opposite
TIME   monsoon evening, steady rain, streetlights just on
CAST   Ravi — 60s tea vendor, grey moustache, calm low voice

SCENE 1 at tea-stall
  RAVI: You're late again.

SHOT 1  medium two-shot from the bus stop, slow push-in on Ravi's line
```

## Contributing

Ideas, issues and discussion are welcome now. We'll accept code contributions once
the contribution policy (CLA or DCO) is settled; see the open decisions in the
development plan.

## Licence

[GNU Affero General Public License v3.0 or later](LICENSE).

**Films, images, audio, worlds and scripts you make with the studio are yours.** The
AGPL covers the software, not your output. See [VISION.md](VISION.md#licence).
