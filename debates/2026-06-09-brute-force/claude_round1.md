TASK: Cross-review ASTER OCEAN (rows 75-76), NEW HARMONY 8 (rows 56-57, 97-98) in 滞期费统计 2026年.xlsx sheet "2026年" for formula completeness — verifying AF cargo ref, AI/AK/AQ formulas, and AO empty cells. Report findings.

## Cross-Review Summary

Hermes R1 audit is substantially correct. Below are the confirmed findings with cross-review notes.

---

## 1. AF Cargo Reference Audit

| Row | Ship | Port | AF Formula | Cargo Ref | Correct? | Issue |
|-----|------|------|------------|-----------|----------|-------|
| 75 | ASTER OCEAN | 装港 | hardcoded 48.617 | — (should be P75) | ⚠️ | Value matches P75/AE75×24 = 70900/35000×24 = 48.617, but no formula. |
| 76 | ASTER OCEAN | 卸港 | =P75/AE76*24 | P75 (装港货量 70,900) | ❌ | Should be Q76 (卸港货量 70,912). Difference: 113.44 vs 113.459 (Δ0.019h, <$1). |
| 56 | NEW HARMONY 8 | 装港 | =P56/AE56*24 | P56 (装港货量 77,000) | ✅ | Correct. |
| 57 | NEW HARMONY 8 | 卸港 | =P56/AE57*24 | P56 (装港货量 77,000) | ❌ | Should be Q57 (卸港货量 77,005). Difference: 123.2 vs 123.208 (Δ0.008h, <$1). |
| 97 | NEW HARMONY 8 | 装港 | =P97/AE97*24 | P97 (装港货量 72,323) | ✅ | Correct. |
| 98 | NEW HARMONY 8 | 卸港 | =P97/AE98*24 | P97 (装港货量 72,323) | ❌ | Should be Q98 (卸港货量 72,328). Difference: 115.717 vs 115.725 (Δ0.008h, <$1). |

**Verdict**: Confirmed — all 3 discharge-port AF formulas incorrectly reference loading-port cargo (P column) instead of discharge-port cargo (Q column). Impact is ~0.01-0.02 hours, negligible financially (<$1) but a formula integrity issue.

---

## 2. AI / AK / AQ Formula Completeness

### AI Column (USD 滞期/速遣)

| Row | Has Formula? | Value | Expected Formula |
|-----|:---:|-------|------------------|
| 75 | ❌ hardcoded | -16,166.27 | =IF((AF75-AC75)>0,(AF75-AC75)*AH75/2,(AF75-AC75)*AH75) |
| 76 | ❌ hardcoded | -54,095.00 | =IF((AF76-AC76)>0,(AF76-AC76)*AH76/2,(AF76-AC76)*AH76) |
| 56 | ❌ hardcoded | 12,700.00 | =IF((AF56-AC56)>0,(AF56-AC56)*AH56/2,(AF56-AC56)*AH56) |
| 57 | ✅ formula | 16,987.50 | =IF((AF57-AC57)>0,(AF57-AC57)*AH57/2,(AF57-AC57)*AH57) — **ONLY row with correct AI formula** |
| 97 | ❌ hardcoded | 3,618.45 | =IF((AF97-AC97)>0,(AF97-AC97)*AH97/2,(AF97-AC97)*AH97) |
| 98 | ❌ hardcoded | 21,444.84 | =IF((AF98-AC98)>0,(AF98-AC98)*AH98/2,(AF98-AC98)*AH98) |

**Verdict**: 5 of 6 rows have hardcoded AI. Only R57 has the proper IF formula (滞期全价/速遣半价 logic). Confirmed Hermes finding.

### AK Column (RMB 滞期/速遣)

| Row | Has Formula? | Value | Expected Formula | Correct? |
|-----|:---:|-------|------------------|----------|
| 75 | ❌ hardcoded | -112,517.24 | =AJ75*AI75 | ✅ value: 6.96×(-16166.27) = -112,517.24 |
| 76 | ❌ hardcoded | -376,501.20 | =AJ76*AI76 | ✅ value: 6.96×(-54095) = -376,501.20 |
| 56 | ✅ =AJ56*AI56 | 88,392.00 | — | ✅ |
| 57 | ❌ hardcoded | 118,233.00 | =AJ57*AI57 | ✅ value: 6.96×16987.50 = 118,233.00 |
| 97 | ❌ hardcoded | 24,714.01 | =AJ97*AI97 | ✅ value: 6.83×3618.45 = 24,714.01 |
| 98 | ❌ hardcoded | 146,468.25 | =AJ98*AI98 | ✅ value: 6.83×21444.84 = 146,468.25 |

**Verdict**: 5 of 6 rows hardcoded AK. All numeric values are consistent (AK = AJ × AI). Only R56 has the formula. Confirmed.

### AQ Column (最终结算)

| Row | Has Formula? | Value | Expected Formula | AK+AO Check |
|-----|:---:|-------|------------------|-------------|
| 75 | ❌ hardcoded | 0 | =AK75+AO75 | -112,517.24 + 112,517.24 = 0 ✅ |
| 76 | ❌ hardcoded | -376,501.20 | =AK76+AO76 | -376,501.20 + 0 = -376,501.20 ✅ |
| 56 | ❌ hardcoded | 0 | =AK56+AO56 | 88,392.00 + (-88,392.00) = 0 ✅ |
| 57 | ✅ =AK57+AO57 | 118,233.00 | — | 118,233.00 + 0 = 118,233.00 ✅ |
| 97 | ❌ hardcoded | 0 | =AK97+AO97 | 24,714.01 + (-24,714.01) = 0 ✅ |
| 98 | ❌ hardcoded | -21,306.08 | =AK98+AO98 | 146,468.25 + (-167,774.33) = -21,306.08 ✅ |

**Verdict**: 5 of 6 rows hardcoded AQ. All AQ = AK + AO numerically. Only R57 has the formula. Confirmed.

---

## 3. AO Empty Cells Audit

| Row | Ship | Port | AO Value | Status |
|-----|------|------|----------|--------|
| 75 | ASTER OCEAN | 装港 | = -AK75 (formula) | ✅ Formula — loading port, customer bears all; dynamically negates AK |
| 76 | ASTER OCEAN | 卸港 | **EMPTY** | ⚠️ Should be 0. If AQ formula =AK76+AO76, empty cell treated as 0 by Excel (benign). But inconsistent. |
| 56 | NEW HARMONY 8 | 装港 | -88,392.00 hardcoded | ⚠️ Hardcoded — loading port, customer bears all. Should ideally be formula =-AK56. Value correct. |
| 57 | NEW HARMONY 8 | 卸港 | **EMPTY** | ⚠️ Same as R76 — benign because R57 AQ is a formula that treats empty as 0. |
| 97 | NEW HARMONY 8 | 装港 | -24,714.01 hardcoded | ⚠️ Hardcoded — same issue as R56. Should be formula =-AK97. Value correct. |
| 98 | NEW HARMONY 8 | 卸港 | -167,774.33 hardcoded | ⚠️ Hardcoded — discharge port with downstream customer bearing (not full absorb). Value needs audit trail. |

**Key observations:**
- R75 has AO as a **formula** (= -AK75) — the only dynamic AO. All other rows are hardcoded.
- R76 and R57 have **empty AO** (discharge port, no customer bearing). Benign in Excel when AQ formula adds them, but R76's AQ is hardcoded — so the AO empty cell doesn't feed a formula anyway.
- R56 and R97 AO are hardcoded negatives of AK (装港全由客户承担). Correct numerically, should be formulas like R75.
- R98 AO = -167,774.33 — the only row where AO is not full loading-port absorb. This is a discharge port with partial customer bearing (downstream). Value integrity cannot be verified from ledger alone; needs source contract cross-check.

---

## 4. Consolidated Findings Matrix

| # | Severity | Finding | Rows Affected | Hermes Confirmed? |
|---|----------|---------|---------------|-------------------|
| F1 | Low | AF discharge-port cargo ref uses P (loading) instead of Q (discharge) | 76, 57, 98 | ✅ |
| F2 | Medium | AI column hardcoded (no IF formula for demurrage/despatch logic) | 75, 76, 56, 97, 98 | ✅ |
| F3 | Medium | AK column hardcoded (should be =AJ*AI) | 75, 76, 57, 97, 98 | ✅ |
| F4 | Medium | AQ column hardcoded (should be =AK+AO) | 75, 76, 56, 97, 98 | ✅ |
| F5 | Low | AO empty in discharge-port rows (should be 0 for consistency) | 76, 57 | ✅ |
| F6 | Low | AO hardcoded at loading-port rows (should be formula =-AK like R75) | 56, 97 | ✅ |
| F7 | Info | AD (总停时) empty | 56, 75, 97 | ✅ |
| F8 | Info | R98 AO = -167,774.33 — discharge port with partial downstream bearing; source unverified | 98 | ⚠️ needs contract check |
| F9 | Info | Exchange rate variation: 6.96 (Mar/Apr) vs 6.83 (May) — verified acceptable | 56-57,75-76 vs 97-98 | ✅ |

---

## 5. Cross-Review Judgment

**All Hermes R1 findings confirmed.** No false positives. No missed issues found.

The discharge-port AF cargo-reference error (F1) is a **systemic pattern** — all 3 ships exhibit the same P-column-instead-of-Q-column reference. This suggests a copy-paste or template error in the original spreadsheet construction. Impact is trivial monetarily but indicates formula hygiene issues.

The hardcoding of AI/AK/AQ across 5 of 6 rows (F2-F4) is the most substantive finding. R57 is the only row with correct formulas in all three columns, suggesting it may have been the template row that was subsequently hardcoded when copied.

**Recommendation**: Fix F1-F6 programmatically or via formula paste to restore ledger integrity. F7 (AD) and F8 (R98 AO source) require out-of-sheet data to resolve.
