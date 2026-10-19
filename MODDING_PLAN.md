# Toy Story 2 continuation plan

- Host: Windows x64; native 32-bit PC original, 1999-10-29 executable.
- Install: C:\Program Files\Disney Interactive\Toy Story 2.
- Engine: original Traveller's Tales nu3d; existing project uses Rust/wgpu/winit/rodio.
- Route: continue original Rust code derived from understood behavior; runtime assets
  supplied by the user's own install. No loader or executable modifications needed.
- Lab: source-only changes, real-data integration tests. Preserve game install/saves.
  The project handoff assigns visual testing to the user; do not auto-launch the viewer.
- Core level data: all 15 available; see docs/re/asset-audit.md and campaign.rs.
- Remaining: level 11/13/14/15 scripts, gaps in previous levels, visual validation of
  level 12, audio/video integration and frontend/progression/save support.
