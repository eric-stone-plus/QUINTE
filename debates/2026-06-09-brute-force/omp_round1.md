The report is written to `/tmp/quinte-audit/omp_round1.md`.

**Summary of findings:**

1. **AK = AJ × AI**: R98 has a **¥0.01 discrepancy** — 6.83 × 21,444.84 = 146,468.2572, rounding to 146,468.26, but the ledger shows 146,468.25. The hermes audit incorrectly claims all rows pass this check. All other 5 sub-rows pass.

2. **AQ = AK + AO**: All rows are internally self-consistent. R98's AQ (-21,306.08) is consistent with its wrong AK; with corrected AK, AQ becomes -21,306.07, cascading the ¥0.01 error.

3. **AF cargo tonnage references**: Three 卸港 rows (R76, R57, R98) incorrectly reference 装港 cargo (P column) instead of 卸港 cargo (Q column). All 装港 rows are correct. Confirmed consistent with hermes findings.

(rounding) |
| R76 | -54,095.00 | 6.96 | -376,501.20 | -376,501.20 | 0.0000 | ✅ PASS |
| R56 | 12,700.00 | 6.96 | 88,392.00 | 88,392.00 | 0.0000 | ✅ PASS |
| R57 | 16,987.50 | 6.96 | 118,233.00 | 118,233.00 | 0.0000 | ✅ PASS |
| R97 | 3,618.45 | 6.83 | 24,714.01 | 24,714.01 | -0.0035 | ✅ PASS (rounding) |
| **R98** | **21,444.84** | **6.83** | **146,468.26** | **146,468.25** | **-0.0072** | **❌ FAIL: ¥0.01 discrepancy** |

### R98 detail
```
AJ × AI = 6.83 × 21,444.84 = 146,468.2572
Rounded to 2dp: 146,468.26 (third decimal = 7 ≥ 5)
Reported AK:     146,468.25  ← off by ¥0.01
```
The hermes audit's overall conclusion states "所有行 AK = AJ × AI ✅" — this is **incorrect** for R98. A ¥0.01 hardcoding error exists regardless of whether AK is a formula or a manually-entered value.

## RESULTS: AQ = AK + AO

| Row | AK | AO | AQ (computed) | AQ (reported) | Δ | Verdict |
|-----|----|----|--------------|---------------|---|---------|
| R75 | -112,517.24 | 112,517.24 | 0.00 | 0.00 | 0.00 | ✅ PASS |
| R76 | -376,501.20 | 0.00¹ | -376,501.20 | -376,501.20 | 0.00 | ✅ PASS |
| R56 | 88,392.00 | -88,392.00 | 0.00 | 0.00 | 0.00 | ✅ PASS |
| R57 | 118,233.00 | 0.00¹ | 118,233.00 | 118,233.00 | 0.00 | ✅ PASS |
| R97 | 24,714.01 | -24,714.01 | 0.00 | 0.00 | 0.00 | ✅ PASS |
| R98 | 146,468.25 | -167,774.33 | -21,306.08 | -21,306.08 | 0.00 | ⚠️ INTERNALLY CONSISTENT but AK base is wrong |

¹ AO empty in source; treated as 0.

**R98 cascade**: AQ = AK + AO is internally consistent (-21,306.08), but both AK and AO are hardcoded. If AK is corrected to 146,468.26:
```
AK     = 146,468.26
AO     = -167,774.33
AQ     = -21,306.07  (not -21,306.08)
```
This means **both AK and AQ are ¥0.01 off** from the true AJ×AI product. The hermes audit did not detect this because it verified AQ=AK+AO self-consistency but not AK=AJ×AI for R98.

## RESULTS: AF CARGO TONNAGE REFERENCES

| Row | Type | Cargo used | Should use | AF (reported) | AF (correct) | Δh | Verdict |
|-----|------|-----------|------------|--------------|-------------|-----|---------|
| R75 | 装港 | P75: 70,900 | P75: 70,900 | 48.617 | 48.617 | 0.000 | ✅ PASS |
| **R76** | **卸港** | **P75: 70,900** | **Q76: 70,912** | **113.440** | **113.459** | **-0.019** | **❌ WRONG COLUMN** |
| R56 | 装港 | P56: 77,000 | P56: 77,000 | 61.600 | 61.600 | 0.000 | ✅ PASS |
| **R57** | **卸港** | **P56: 77,000** | **Q57: 77,005** | **123.200** | **123.208** | **-0.008** | **❌ WRONG COLUMN** |
| R97 | 装港 | P97: 72,323 | P97: 72,323 | 115.717 | 115.717 | 0.000 | ✅ PASS |
| **R98** | **卸港** | **P97: 72,323** | **Q98: 72,328** | **115.717** | **115.725** | **-0.008** | **❌ WRONG COLUMN** |

All 3 卸港 rows reference 装港 cargo (P column) instead of 卸港 cargo (Q column). All 3 装港 rows reference correct cargo. Confirmed consistent with hermes audit findings. Financial impact negligible (< $1 per row) but represents a formula integrity defect.

## DISCREPANCY INVENTORY

| # | Severity | Row | Field | Issue | Financial impact |
|---|----------|-----|-------|-------|-----------------|
| 1 | **MEDIUM** | R98 | AK | 146,468.25 should be 146,468.26 (¥0.01) | ¥0.01 |
| 2 | **MEDIUM** | R98 | AQ | Cascaded from AK error; -21,306.08 should be -21,306.07 | ¥0.01 |
| 3 | LOW | R76 | AF | References P75 (70,900) not Q76 (70,912); Δ=0.019h | < $1 |
| 4 | LOW | R57 | AF | References P56 (77,000) not Q57 (77,005); Δ=0.008h | < $1 |
| 5 | LOW | R98 | AF | References P97 (72,323) not Q98 (72,328); Δ=0.008h | < $1 |

## CORRECTION TO HERMES R1

The hermes audit conclusion "所有行 AK = AJ × AI ✅" is **false**. R98 AK fails AK = AJ × AI by ¥0.01. The hermes audit also missed the cascading effect on AQ. The AF cargo reference findings are confirmed and correct.
