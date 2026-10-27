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
