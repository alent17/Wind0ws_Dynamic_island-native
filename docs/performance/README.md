# Native UI performance and regression evidence

This folder preserves raw measurements produced by the standalone Rust/Win32 application. The reports are grouped by feature and dated build; they are not one continuous benchmark series.

- `native-prototype-2026-09-27/`: initial prototype samples.
- `native-frame-timing/`: frame scheduling, lifecycle, and idle-render measurements.
- `native-display-accessibility/`: display, resource, and accessibility checks.
- `native-artwork/`, `native-audio/`, `native-clock/`, `native-configuration/`, `native-media-integration/`, `native-players/`, `native-settings/`, `native-spectrum/`, and `native-weather/`: feature-specific regression evidence.
- `native-1.0.*`, `live-spectrum-*`, and `spectrum-*`: dated performance snapshots.

Each feature report records its own build hash, fixture, duration, and limitations. Synthetic media and simulated DPI do not establish live-device behavior, real mixed-DPI layout, screen-reader support, or long-duration memory stability. See the project-level [progress table](../native-ui-v2-progress.md), [QA log](../native-ui-v2-qa.md), and [review notes](../native-ui-v2-review.md) for current status.