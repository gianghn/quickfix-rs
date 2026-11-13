# quickfix-fuzz

libFuzzer targets for QuickFIX message parsing (go-live phase 14). They are a separate cargo
workspace, so `cargo build`/`cargo test` at the repository root never see them or `libfuzzer-sys`.

| Target | Parses the fuzzer's bytes with |
|---|---|
| `parse_no_dict` | no dictionary (`Message::try_from_bytes`) |
| `parse_fix44` | `FIX44.xml` |
| `parse_fix44krx` | `quickfix-msgkrx/src/FIX44KRX.xml` |
| `parse_fixt11_fix50` | `quickfix-msg50/src/spec.xml` (FIXT1.1 session + FIX50 application in one file) |

Each one serializes what parsed. The input goes to QuickFIX as raw bytes, non-UTF-8 and NULs
included (`try_from_bytes` / `try_build_message_bytes`, phase 15); the old research harness
decoded it lossily, which shifted `9=`, `95=` and `10=`.

## Run locally

Linux, nightly, clang and `cargo install cargo-fuzz`. Use `scripts/fuzz.sh`, not plain `cargo fuzz`:
it also builds libquickfix (C++) with ASan and libFuzzer's coverage counters. Without them the
fuzzer is blind inside the parser.

```sh
scripts/fuzz.sh run parse_fix44 corpus/parse_fix44 seeds -- -dict=fix.dict -max_total_time=600
```

`corpus/<target>` collects what the fuzzer learns and `seeds/` is read only. A crash is saved
under `artifacts/<target>/`. Replay one with
`scripts/fuzz.sh run parse_fix44 artifacts/parse_fix44/crash-…`.
On macOS, use a Linux container (`KRX_BUILD_PLAN.md`, handovers 1 and 11).

- `seeds/`: 95 research-corpus frames (FIX 4.0–4.4, FIXT1.1), merged down from 3431 by libFuzzer
  `-merge=1` over the four targets, 3 frames with data fields (`data-*`, `95=1|96=x`,
  `354=2|355=de`), and the 4 `r-p0-13-*` frames below.
- `fix.dict`: separators, header tags, every data/length tag pair, group counters and edge values.

## R-P0-13

QuickFIX trusted a data field's length tag (`95=100000|96=x` read 100000 bytes; upstream #779)
until the C++ fork's P0-1 fix, pinned in phase 16. The `r-p0-13-*` seeds are the frames that
crashed it: three hand-made `95=` Logons and the first one this fuzzer found by itself
(`90=6`/`91=`, SecureData). Every run replays them first, and the crate's
`former_r_p0_13_crashes_are_refused_by_the_dictionary` test checks a dictionary refuses them
(without one, QuickFIX knows no data fields and ends each value at the next SOH).
