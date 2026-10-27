# Deck preview fonts

The deck draws every label with LVGL's built-in Montserrat Medium fonts at 48, 28 and 14 px, which lv_font_conv
generated from `Montserrat-Medium.ttf` (Montserrat 7.200) and seven media symbols from
`FontAwesome5-Solid+Brands+Regular.woff` (Font Awesome Free 5.9.0). Both files ship with LVGL in
`scripts/generators/built_in_font`. The preview draws with the same two files, subset to what it needs:

- `montserrat-medium.woff2`: Montserrat Medium, Basic Latin, Latin-1, and a few quotes and dashes. SIL Open Font
  License 1.1, in `LICENSE-Montserrat.txt`.
- `touchdeck-symbols.woff2`: the previous, play, pause, next, mute, volume-down and volume-up glyphs of Font Awesome 5
  Free Solid at their LVGL code points U+F048, U+F04B, U+F04C, U+F051, U+F026, U+F027 and U+F028. SIL Open Font
  License 1.1, in `LICENSE-FontAwesome.txt`. The subset is a modified version, so it is renamed TouchDeck Symbols, as
  the license's Reserved Font Name requires.

The glyph advances and kerning the preview measures with are in `src/lib/deck/fonts.data.ts`, generated from LVGL's
`lv_font_montserrat_<size>.c` by `src/lib/deck/fonts.extract.mjs`.
