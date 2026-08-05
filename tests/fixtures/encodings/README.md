# Encoding fixture provenance

These fixed byte fixtures were encoded independently from UTF-8 source text
with GNU `iconv` from glibc 2.39 (`Ubuntu GLIBC 2.39-0ubuntu8.8`). They are
test inputs, not generated outputs. The UTF-16 source begins with U+FEFF so
the UTF-16LE and UTF-16BE files contain the corresponding byte-order mark.

| File | `iconv` target | SHA-256 |
| --- | --- | --- |
| `utf-16le.txt` | `UTF-16LE` | `89092b865c9a2447b8ea301b974459256ad938874750e44bcecea1ae6296576f` |
| `utf-16be.txt` | `UTF-16BE` | `f6dbc0c548d420a3fa7ebdedfbe73c4275693e895ac5161c52d5126439dd79fd` |
| `shift-jis.txt` | `SHIFT_JIS` | `3f5ea89b27d50f0978035ed513a81f359ba814ad39776fb402b762313d942dbf` |
| `euc-jp.txt` | `EUC-JP` | `91a37bc153ef380393e5c2cb8f52e793e593c5cd1e0d9b7de1cd20c151023d0f` |
| `gbk.txt` | `GBK` | `7fd8f1bcec1064109b0511f69e94d0655a8894f8da29c60f45c03940936bc33e` |
| `gb18030.txt` | `GB18030` | `9e595b6e63720df4393911617670f8c3136f82757fee328b7f550dc12ad95cd4` |
| `big5.txt` | `BIG5` | `193d7f0e99d3a5964ebf217e629efef1c707d2c83be8317d7ec4f81271b91602` |
| `iso-2022-jp.txt` | `ISO-2022-JP` | `5e3a4177b42d3c7f2aaa7a5b48456d2bb0a16ca18acb7df2c902c45382b6888f` |
| `windows-1251.txt` | `WINDOWS-1251` | `bc18e2357afd2a20f107a1c5ac44e3dbc17c9d9a7710369b3e92a7d6bfb5bb95` |
| `windows-1252.txt` | `WINDOWS-1252` | `e4a57b8dc2af3b7865147af1ce6d9d0375d63ca9fa16200e52225b3eb116beb7` |

The Japanese, simplified Chinese, traditional Chinese, Russian, and western
European fixtures contain multiple natural sentences to avoid relying on the
detector's weaker very-short-input behavior. The GB18030 fixture includes
U+20000, which requires the four-byte extension. `chardetng` deliberately
reports GB18030 input with the canonical decoder label `GBK`.
