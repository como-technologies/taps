# Pharmaceutical Market Research Assessment Suite

This directory contains five assessments adapted from pharmaceutical market research survey patterns. Each assessment transforms descriptive survey methodology into evaluative binary checks following the Amaker metamodel.

## Assessments

### Point-in-Time Assessments

| Assessment | Target Audience | Questions |
|------------|-----------------|-----------|
| [Market Research Maturity](pharma-market-research-maturity.md) | Pharma market research teams | ~61 |
| [Physician Practice Readiness](physician-practice-readiness.md) | Practices seeking trial/KOL participation | ~67 |
| [Commercial Launch Readiness](commercial-launch-readiness.md) | Commercial teams preparing for launch | ~71 |
| [Treatment Decision Quality](treatment-decision-quality.md) | Healthcare systems evaluating care quality | ~77 |

### Longitudinal Tracker

| Assessment | Target Audience | Questions | Cadence |
|------------|-----------------|-----------|---------|
| [Competitive Position Tracker](competitive-position-tracker.md) | Brand teams monitoring market dynamics | ~65 | Quarterly |

## Source Material

Derived from pharmaceutical market research surveys across:
- **Therapeutic areas**: Oncology (HER2+ breast cancer, mCRPC prostate cancer), Psychiatry (antipsychotics)
- **Survey types**: Physician practice patterns, treatment sequencing, biomarker utilization, SDOH analysis
- **Data patterns**: Multi-language deployment, chart audits, longitudinal tracking, equity crosscuts

## Survey-to-Assessment Transformation

| Survey Pattern | Assessment Pattern |
|----------------|-------------------|
| "What % of patients receive Drug X?" | "Is treatment selection evidence-based for this indication?" |
| "How many patients do you treat annually?" | "Does patient volume support therapeutic expertise?" |
| "Which biomarkers do you test?" | "Is biomarker testing comprehensive per guidelines?" |
| Descriptive/quantitative | Evaluative/binary |

## Usage

Each assessment follows the standard metamodel structure:
- **Domains**: 4-5 major focus areas
- **Practices**: 2-4 specific capabilities per domain
- **Questions**: 5-10 binary checks per practice with verification guidance

Suitable for:
- Self-assessment by pharmaceutical companies
- Third-party evaluation of research partners
- Clinical trial site qualification
- Commercial readiness gates
- Quality improvement initiatives

---

## Longitudinal Assessment Design

The Competitive Position Tracker introduces patterns for assessments that repeat over time (quarterly, annually). Key concepts:

### Question Stability Tiers

| Tier | Description | Change Frequency | Trending |
|------|-------------|------------------|----------|
| **Core** | Fundamental metrics (share, access, reach) | Rarely changed | Full history |
| **Strategic** | Current priorities (may shift quarterly) | Reviewed each cycle | Rolling 4Q |
| **Exploratory** | One-time deep dives, emerging issues | As needed | Point-in-time |

### Handling Question Evolution

When assessments need to change over time:

**New Questions**: Mark as "New in Qn" - establishes baseline, no prior trend available

**Modified Questions**:
- Document the change and rationale
- Note the trend break point
- Provide bridging estimate if possible (e.g., "new methodology reads ~3pts higher")

**Retired Questions**:
- Archive final reading
- Document reason for retirement
- Link to any replacement questions

**Split Questions**:
- When one question becomes multiple (e.g., "biomarker+" splits into "BRCA+" and "other HRR+")
- Link new questions to parent for historical context
- Prior readings may need restatement if underlying data supports it

### Example: MERK mCRPC Survey Evolution

The source surveys (Q1 2024 → Q2 2024) demonstrate this approach:
- **358 core questions** remained identical for trending
- **2 additions** to specialty eligibility (minor)
- **No retirements** in first evolution
- **Multi-dimensional design** enables segment-level trending even as total market evolves

### Minimal Effort Quarterly Process

1. **Data refresh** (automated where possible): Rx data, CRM metrics, formulary status
2. **Binary threshold check**: Is metric above/below target? Better/worse than prior Q?
3. **Flag exceptions**: Only items needing attention surface for review
4. **Trend update**: Add Q reading to trend line; note any breaks
5. **Action items**: Identify items requiring response before next quarter

This transforms hundreds of data points into a manageable set of yes/no assessments with trend context.
