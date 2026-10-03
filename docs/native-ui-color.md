# Native island background colors

The native settings dialog now reads and saves the existing `floatingFillColor` and `floatingUseAlbumColor` preferences. The background color accepts `#RRGGBB`; malformed values fall back to the existing dark blue-gray default. Changes apply to the island as soon as the asynchronous configuration save succeeds and survive restart.

When album color is enabled and a cover is visible, the renderer picks a saturated color from the bounded, premultiplied BGRA cover pixels. It computes this once when the cover texture changes and caches the result with that texture. Transparent, black, and white pixels are skipped. With no cover, the configured color remains the fallback. Disabling album color uses the configured color for both the island and title fade edges.

The native renderer continues to release its cover bitmap when the music page is hidden. The cached accent therefore has the same bounded lifetime and does not keep artwork alive while the island is collapsed or showing another page.

Verification covers the color parser and configuration round-trip, album-color checkbox save, custom color loaded into the native edit control, rendering accent selection, restart restoration, and the settings dialog on the left `DISPLAY2`. Cross-process UI automation on the non-activated benchmark window does not reliably enter text into a standard Windows edit control, so the test verifies an existing custom value is loaded and preserved while toggling album color; direct typing remains a manual check.
