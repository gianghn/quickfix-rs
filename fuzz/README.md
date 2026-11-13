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
  `-merge=1` over the four targets, and 3 frames with data fields (`data-*`, `95=1|96=x`,
  `354=2|355=de`).
- `fix.dict`: separators, header tags, every data/length tag pair, group counters and edge values.

## R-P0-13 and the `skip-r-p0-13` feature

QuickFIX trusts a data field's length tag (`95=100000|96=x` reads 100000 bytes; upstream #779).
That bug is open, so a nightly run would stop at it every time and find nothing else. The
default feature `skip-r-p0-13` skips inputs where a length field promises more bytes than follow
it. `known_crashes/` holds four frames that crash today: three hand-made `95=` Logons and the
first one the fuzzer found by itself (`90=6`/`91=`, SecureData), all checked by the crate's
`overreads_data_field_returns_true_for_every_known_crash` test. To see them crash, turn the filter off:

```sh
scripts/fuzz.sh run parse_fix44 --no-default-features known_crashes/r-p0-13-95-100000
```

Once the C++ fix is pinned, delete the feature and `overreads_data_field`, and move
`known_crashes/` into `seeds/`.
