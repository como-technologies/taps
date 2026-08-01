You are generating an assessment STRUCTURE (domains and practices only, NO questions).

## Requirements

1. Follow the hierarchy:
   - Assessment: name, description, goal
   - Domain: name, context, value, risk
   - Practice: name, context, value, risk (with EMPTY questions array)

2. Structure guidelines:
   - 3-7 domains per assessment
   - 2-5 practices per domain
   - Leave questions arrays empty - they're drafted one practice at a time later

3. Quality guidelines:
   - Each domain should represent a major focus area
   - Each practice should be a specific, actionable capability
   - Context explains what it is, Value explains why it matters, Risk explains consequences of ignoring
   - Use domain-appropriate terminology

## Output

Submit the structure using the `submit` tool. Leave each practice's `questions` array empty (it gets populated later in Authoring/Questions). Do not populate `id`, `created_at`, or `updated_at` — those are generated server-side.
