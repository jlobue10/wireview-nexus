# v2.1.1 audit follow-up

Audited tag `v2.1.1`, commit `e50baec54225404f6a840c32a5c9926d4e36e28b`, with companion core commit `7128fe49e24f401381e467400d1c413cda1a2538`, on 2026-10-01 UTC.

## New findings

| ID | Severity | Trigger and effect | Fix |
| --- | --- | --- | --- |
| WV-11 | Medium | The combined layout places full device-fault names over the power reading. `WIRE OVER-CURRENT` overlaps it by 32 pixels with the tested system font; all six active faults extend off-screen and cover several readings. The other layouts also share footer space with long fault text. | Reserve the footer for concise, explicitly labelled device faults in every layout. Keep readings and a short `DEVICE FAULT` status visible, and move supporting text above the footer. |
| WV-12 | Low | A binary beside a copied installer suppresses downloads even when `-Dir` selects another folder. The requested release and checksum can be ignored while a destination binary is missing or outdated. | Use in-place mode only for the script's own destination. Cover empty and existing alternative destinations with installer regressions. |
| WV-13 | Low | The CLI accepts arbitrarily large positive `--fps` values. `1e10` rounds the frame period to zero, removing the intentional sleep; `0.1` is silently run at `0.2`. This requires a user's configuration and is a performance/configuration bug. | Accept 0.2 through 60 fps, document the range, reject excessive/sub-minimum rates and keep a nonzero period. The 2 fps default is unchanged. |

The rendering tests check all 63 non-empty fault combinations across all four layouts: faults cannot cover the numeric readings. A separate pixel check proves the full six-fault list fits on the reserved row with the loaded system font. The per-wire limit caption is shortened to avoid competing with the sixth pin's watts.

## Verification

- Released tag: 23 Rust/CLI tests, formatting, Clippy and PowerShell installer regressions passed on Linux. The tagged commit's [Windows/Linux CI](https://github.com/jlobue10/wireview-nexus/actions/runs/36781615933) passed.
- Follow-up: 26 Rust/CLI tests, formatting, Clippy and the expanded installer cases passed on Linux.
- `cargo audit` 0.22.2 checked 95 locked packages against RustSec commit `9b3a3b73a7f42606494c943e95f8196e9994df46`: zero vulnerabilities and zero warnings. This follow-up changes no dependencies.
- Published executable SHA-256: `971bae18ac0a5315e321959cb8b6a7b489f3ffabce492ae4343d69908a27bebf`, matching the release checksum. GitHub CLI verified its signed provenance against this repository's release workflow at this exact tag and commit, rejecting self-hosted runners.
- The release installer matches the tagged source after normalizing Windows line endings.

The earlier total-limit/cable-limit rendering, Windows HID backend and maintained font-parser changes are present; their regressions pass. No new confirmed security vulnerability was found in the reviewed scope.

## Performance and limits

An optimized Linux harness, including the tagged production renderer, measured approximately 0.13-0.22 ms per frame over 200 iterations per layout. This is a host/font-specific render-only measurement, not a Windows or USB throughput result. Packet tests confirm 121 reports of 1,024 bytes per full frame: at the default 2 fps that is 247,808 bytes and 242 HID writes per second, before bus overhead. There is no dirty-frame suppression.

No physical Nexus, WireView or Windows desktop was available for the independent checks. The repository records the owner's successful physical Windows Nexus test of v2.1.0; v2.1.1 does not change its daemon behavior, but this audit did not independently repeat that test. HWiNFO ordering and Windows Task Scheduler/ACL behavior remain platform-specific validation limits. Nexus release provenance identifies this repository; companion core is selected by the matching tag in the workflow rather than an immutable SHA embedded in the manifest.
