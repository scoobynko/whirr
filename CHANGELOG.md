# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## v0.3.7

_2026-08-07 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.6...v0.3.7)_


### Added

- *(ui)* Settings dialog for theme, accent, background and fan ([62d1ec5](https://github.com/scoobynko/whirr/commit/62d1ec51847d173dd67f7ea4b24f339f1100879f))
- *(settings)* Remember the choices between runs ([dfdd8ce](https://github.com/scoobynko/whirr/commit/dfdd8ce44cfe45774744ff1d7e9806ecdd0cd2d5))

### Changed

- *(ui)* Make the palette a value instead of eleven constants ([1efef65](https://github.com/scoobynko/whirr/commit/1efef65bd10e963339209f0d79c266f7972deb7f))

### Fixed

- *(settings)* Terminal background is dark-palette only ([0455a0b](https://github.com/scoobynko/whirr/commit/0455a0bd0d1145e9729e85d906dc75c5fbce0afc))
## v0.3.6

_2026-08-07 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.5...v0.3.6)_


### Added

- Tell the user when a newer release exists ([f169b11](https://github.com/scoobynko/whirr/commit/f169b11cfb79945d1164f8b11680c9253cadb890))
- *(ui)* Show the running version in the header ([778959e](https://github.com/scoobynko/whirr/commit/778959e731249f57b67be9b67ad3910c93f57b6d))
## v0.3.5

_2026-08-07 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.4...v0.3.5)_


### Added

- *(sessions)* Show the tty only when a project has two sessions ([455a728](https://github.com/scoobynko/whirr/commit/455a7282656c51386740ca0cd32d86568e0d9dd7))
## v0.3.4

_2026-08-07 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.3...v0.3.4)_


### Fixed

- *(ui)* Make the kill confirmation a dialog, not a line in another card ([d7c51bf](https://github.com/scoobynko/whirr/commit/d7c51bf0566f8ad562110fa8e914eaeaf73806a3))
- *(ui)* Cap dialog width and cover the modal path at every size ([bc43759](https://github.com/scoobynko/whirr/commit/bc437594f57dbc4473eb803e49649629c13b3500))
## v0.3.3

_2026-08-06 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.2...v0.3.3)_


### Added

- *(ports)* Open the selected dev server with o ([a068079](https://github.com/scoobynko/whirr/commit/a068079e3e053dc6bdf4bc5378243305f6c578f2))

### Fixed

- *(ports)* Find the repo above a dev server's cwd, not just at it ([e1a6418](https://github.com/scoobynko/whirr/commit/e1a6418a5030f3414976d7a917c608add6f4de62))
- *(ui)* Let the process table fill the height it is given ([faf6bdf](https://github.com/scoobynko/whirr/commit/faf6bdf770802326b1125e0835084ec9677b8b0a))
## v0.3.2

_2026-08-02 · [compare](https://github.com/scoobynko/whirr/compare/v0.3.1...v0.3.2)_


### Documentation

- Unwrap the demo video link so GitHub embeds it ([690bb3a](https://github.com/scoobynko/whirr/commit/690bb3a4ac9450217096408fec1dd06e1c9ad3ca))

### Fixed

- Title GitHub Releases v0.3.1, not "0.3.1 - 2026-08-02" ([014c88c](https://github.com/scoobynko/whirr/commit/014c88cdcbe873d3bdee152869376e19a3a8e4ca))

## [0.3.1](https://github.com/scoobynko/whirr/compare/v0.3.0...v0.3.1) - 2026-08-02

### Documentation

- add screenshot link in README

## [0.3.0](https://github.com/scoobynko/whirr/releases/tag/v0.3.0) - 2026-07-31

### Added

- scaffold whirr crate with safe terminal setup
- byte/rate/duration formatting
- fixed-capacity history ring buffer
- sysctl helpers and static system info
- snapshot types and fast sampler (cpu/procs/net)
- memory detail via host_statistics64 + pressure sysctl
- battery via AppleSmartBattery IORegistry
- CPU/GPU/ANE power via IOReport Energy Model
- sudo-free CPU temperature via IOHID sensors + --list-sensors
- medium sampler (temp/power/battery/memory)
- slow sampler parses listening ports from lsof
- app state, key handling, kill confirm, fan timing
- theme with magnitude gradient and status colors
- header with block logo and load-driven fan animation
- CPU panel with P/E core heatmap and braille history
- temperature thermometer with status-colored trend
- power panel with hero watts, stacked energy chart, battery
- memory panel with pressure headline and segmented bar
- mirrored network waveform panel
- process table with micro-bars and kill flow
- ports panel with badges and stale marker
- responsive collapse + TestBackend render suite; split lib/bin
- cwd_basename via PROC_PIDVNODEPATHINFO
- ports card shows owning project folder
- cap process table at top 10, grow ports card
- move ports card into left column under processes
- render one port per line with scroll-into-view
- smart port labels via KERN_PROCARGS2
- 4-row tall-rounded hero font with hero_fits predicate
- fan ticks 8 frames at half interval for smoother spin
- tiered header with 4-row logo and housed 8-frame fan
- power card hero in 4-row font with compact fallback
- cpu card hero with per-core color strip
- temp card hero replaces thermometer at full size
- memory card hero with consolidated pressure/swap line
- switch layout to full visual tier at >=120x30
- replace housed fan with 8-arm star fan per visual reference
- star fan rotates via traveling gap in brand color
- star fan rotates continuously via per-frame rasterization
- fan is a windmill of star arms flipping between + and x
- windmill fan at font height with double-asterisk blades, slower flip
- hub asterisks in the windmill x state
- fan is a 17x7 continuously rotating asterisk
- two-tone fan arms and thermal fan-curve speed
- block-sparkline chart helper
- CPU history as block sparkline
- Temp history as baseline-shifted block sparkline
- Power total as block sparkline with cpu/gpu/ane legend
- Network as two stacked block-sparkline bands
- rgb blend helper for burst fan anti-aliasing
- two-ring braille burst rasterizer
- continuous thermal fan angle replaces the frame counter
- drive the fan from real elapsed time
- burst fan fills the full header band
- burst fan sits 19x7 centred in the header band
- purpose-classification and per-process rows for ports
- slow sampler emits grouped port rows; drop the argv label heuristic
- ports card renders three purpose groups
- kill dev servers from the ports card, localhost rows only
- cheap exec-path, tty and pid-enumeration readers
- pure Claude session row building, sourced from processes
- slow sampler enumerates Claude sessions from processes
- focus and kill across four panels
- three side-by-side cards with a process-sourced sessions card
- raise the three-card band's height cap to 8 content rows
- Paint a near-black BASE background across the whole frame
- Sparkline bars ramp by height instead of one flat colour
- Glow halo on the burst fan's ray fringe
- Depth on the hero numbers via edge/interior colouring
- quadrant hero font transcribed from FIGlet smblock
- wordmark uses the quadrant font too
- drop per-core CPU load from both tiers
- 2x2 gauge grid so narrow-but-tall terminals keep the hero design
- compact tier shares the burst fan and bitmap wordmark

### Changed

- mac::proc::cwd returns the full working directory

### Documentation

- whirr implementation plan
- README, MIT license; release install
- note QoS caveat on perf measurements
- v0.2 tweaks design spec (top-10 processes, port project info)
- v0.2 tweaks implementation plan
- v0.2 layout revision — ports under processes
- smart port labels design spec
- visual refresh design spec
- visual refresh implementation plan + width-gate spec amendment
- header full tier needs 7 rows, not 5
- spec for block-sparkline history charts
- implementation plan for sparkline charts
- note sparkline charts in visual-refresh design
- spec for the braille burst fan
- implementation plan for the braille burst fan
- add wispiness check to the burst fan font gate
- revise burst fan to counter-rotating ray halves
- correct Task 4's impossible verification step
- carry the repaired interval tests into Task 6 verification
- burst sized to 19x7 centred in the header band
- fix stale star-fan comments and a reverted-size spec table row

### Fixed

- clamp sysctl_string length to buffer size
- clamp battery percent and guard null matching dict
- release IOReport subscribed dict, document teardown policy
- document HID client teardown, guard Product property type
- include top-memory processes in sampler snapshot; session-relative network totals
- wrap CPU heatmap onto multiple rows
- treat empty lsof result as valid, not stale
- graceful middle-row collapse at small heights, derive layout from MAX_VISIBLE_PROCS
- default SIGPIPE so piped --list-sensors doesn't panic
- restore % glyph diagonal per plan
- gate full header tier at 7 rows to keep housing unclipped
- pin e_cores in cpu tests instead of widening demo fixture
- hero strings fall back to coarse precision instead of truncating
- Update stale fan_interval() test expectations to new thermal curve
- clamp tick_fan's per-frame step to the 17° alias limit
- Replace vacuous dt=10s case with dt=7s in fan clamp test
- right-anchor short sparklines and clamp overflow values
- memory card wrap, hero_lines helper, NaN heat guard, honest layout comment
- Add pid as tertiary sort key for deterministic port row ordering
- ports scroll offset, claude row alignment, and truncation test
- Assert ports card scroll invariant to catch future coupling breaks
- *(ports)* detect Claude sessions by launcher exec path, not versions dir
- budget ports card row layout against terminal width
- Ports card rendering defects — remove % from unknown CPU, prevent doubled ellipsis
- remove redundant group markers from single-group port cards
- Rebalance card band layout from Min(5)/Length(6) to Min(4)/Max(6)
- Network card never drops its rate readout
- Lower processes floor from Min(4) to Min(3) to recover card row height
- Floor the sparkline ramp so low bars don't vanish into BASE
- replace battery emoji with single-width arrows
- render hero digits and wordmark as bg-filled cells, not fg glyphs
- 5-row bitmap glyphs for legible hero digits
- recover the Memory card's usage bar at 120-col width
- selected process row's highlight spans its micro-bars
- paint full sparkline cells as backgrounds, not stacked block glyphs

### Performance

- replace sysinfo process refresh with direct libproc scan
