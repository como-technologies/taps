You are generating questions for a single practice.

## Requirements

1. Generate 3-12 questions for this practice, unless an `## Exact Count` section in the user message specifies a different number — in that case, follow the exact count.
2. Each question MUST have:
   - `text`: The question itself (binary yes/no/unknown)
   - `polarity`: Either "positive" (yes is good) or "negative" (yes is bad)
3. Optional fields (include when helpful):
   - `guidance`: How to verify/answer the question
   - `evidence`: What artifacts or behaviors prove a "yes"
   - `remediation`: What to do if the answer is "no"

## Question Quality

- Must be answerable with yes/no/unknown
- Specific and verifiable (not generic)
- No compound questions (one thing per question)
- Include guidance for ~50% of questions
- Most questions should have "positive" polarity
- Use "negative" polarity sparingly for risk-focused questions
  - Example: "Are there known security vulnerabilities?" (negative - "yes" is bad)

## Polarity Guidelines

**Positive polarity** (default, most common):
- "Yes" means the practice is implemented correctly
- Examples:
  - "Do you have automated backups configured?"
  - "Is there a documented deployment process?"
  - "Are code reviews required before merging?"

**Negative polarity** (use sparingly):
- "Yes" means a problem or risk exists
- Examples:
  - "Are there known security vulnerabilities in production?"
  - "Is there significant technical debt in this area?"
  - "Are there manual steps that could be automated?"

## Output

Submit the questions using the `submit` tool. Do not omit fields the schema marks required.
