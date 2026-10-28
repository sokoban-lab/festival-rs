# Microban I

155 compact Sokoban levels by **David W. Skinner**, first published in 1999 and
widely redistributed as freeware/public-domain level data. They are used here
as the Gate G2 regression corpus and as the small-level baseline for the C++
differential runs.

- File: `microban.xsb` (155 levels, `; Microban N` headers)
- Source used for the snapshot:
  <https://github.com/thisisandreeeee/sokoban-solver/blob/main/sokoban/levels/microban.xsb>
- SHA-256: `7077dba071244d5e47d212d715b50ad5f815d67125823c78d83b428ae37280d5`

`scripts/fetch_corpus.sh` downloads the same file and verifies the checksum;
the vendored copy keeps tests runnable offline. No changes were made to the
level data.

# XSokoban

90 screens from **XSokoban 3.3c** by Andrew Myers (50 original Thinking
Rabbit levels plus 40 public-domain screens). XSokoban is distributed in the
public domain and may be freely redistributed.

- File: `xsokoban.sok` (90 levels, `Title: XSokoban N` headers)
- Source: <https://github.com/andrewcmyers/xsokoban/tree/master/screens>
- SHA-256: `0c6fe8d143a1ac22b961a2c4e201349cd2c3901d7b8c939d748790d402f9a60a`

`scripts/fetch_xsokoban.sh` downloads `screens/screen.{1..90}`, joins them into
one `.sok` file and verifies the checksum. Gate G6 uses levels 1–30 as the
XSokoban subset.
