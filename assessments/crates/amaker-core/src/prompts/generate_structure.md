You are generating an assessment STRUCTURE (domains and practices only, NO questions).

## Requirements

1. Follow the hierarchy:
   - Assessment: name, description, goal
   - Domain: name, context, value, risk
   - Practice: name, context, value, risk (with EMPTY questions array)

2. Structure guidelines:
   - 3-7 domains per assessment
   - 2-5 practices per domain
   - Leave questions arrays empty - they will be added in the next phase

3. Quality guidelines:
   - Each domain should represent a major focus area
   - Each practice should be a specific, actionable capability
   - Context explains what it is, Value explains why it matters, Risk explains consequences of ignoring
   - Use domain-appropriate terminology

## Output Format

Output the structure as a YAML code block. Do NOT include id, created_at, or updated_at fields - these are generated automatically.

```yaml
name: "Assessment Name"
description: "What this assessment evaluates"
goal: "Intended outcome"

domains:
  - name: "Domain Name"
    context: "What this domain covers"
    value: "Benefits of addressing this well"
    risk: "Consequences of ignoring"
    practices:
      - name: "Practice Name"
        context: "What this practice is"
        value: "Specific benefits"
        risk: "Specific consequences"
        questions: []  # Empty - questions added in next phase
      - name: "Another Practice"
        context: "..."
        value: "..."
        risk: "..."
        questions: []
  - name: "Next Domain"
    context: "..."
    value: "..."
    risk: "..."
    practices:
      # ... more practices with empty questions
```
