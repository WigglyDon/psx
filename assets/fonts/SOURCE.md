# Native UI fonts

## Barlow Semi Condensed Medium

Unmodified `ofl/barlowsemicondensed/BarlowSemiCondensed-Medium.ttf` from
[google/fonts](https://github.com/google/fonts/tree/3218ae2acc5bce7ccd97c477965b5b4094241fb0/ofl/barlowsemicondensed),
revision `3218ae2acc5bce7ccd97c477965b5b4094241fb0`.

Copyright 2017 The Barlow Project Authors (https://github.com/jpt/barlow).
SIL Open Font License 1.1; exact upstream license retained as
[Barlow-OFL.txt](Barlow-OFL.txt) and in APK notices.

Font SHA-256:
`4998b693a190b76f2b11315844345a8546c1e2e6a320905507a9eed04edf8b48`.
License SHA-256:
`186d750eb496a4c17a76385f82be6aea2ac1cf2de074a811d63786cf374ea73f`.

The compact medium face supplies the embedded menus and control labels.
Noto Sans supplies supported atlas characters absent from Barlow. Both feed
the existing single bounded atlas; no runtime font service is used.

## Noto Sans Regular (fallback)

Unmodified `hinted/ttf/NotoSans/NotoSans-Regular.ttf` from
[notofonts/noto-fonts](https://github.com/notofonts/noto-fonts), revision
`ffebf8c1ee449e544955a7e813c54f9b73848eac`.

Copyright 2018 The Noto Project Authors. SIL Open Font License 1.1; the exact
upstream root license is retained as [LICENSE](LICENSE) and in APK notices.

Font SHA-256:
`b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5`.
License SHA-256:
`0dab92d0544f7b233403f14b84a663bdbfa746982eda629e7f4f9ffe1b036feb`.

The Rust host rasterizes one bounded Latin/punctuation atlas through
`ab_glyph` for its embedded menus and control labels. The font file is
unchanged. Unsupported display glyphs use a placeholder; stored file paths
retain their original Unicode.
