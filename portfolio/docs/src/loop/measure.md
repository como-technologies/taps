# Measure

Adoption is observed, not assumed — on two axes. One instrument counts
where engineering capacity actually goes; the other hears what people
won't say in a town hall. Both write their results into the knowledge
base beside the decisions they measure, and both feed the next
assessment — which is what makes the loop a loop.

## tuesday

Team capacity analysis from merged pull requests. Developers self-report
relative effort on each PR with a label; tuesday turns that into monthly
capacity breakdowns and attributes the hours to the decision each PR
implements. You see where engineering time actually went — per category
and per decision — not where it was planned to go.

**What you get.** A monthly capacity report, as an interactive page or
plain JSON, that answers the question every steering committee asks and
few can: *what did this decision cost us?*

Details live in the
[tuesday repo](https://github.com/como-technologies/tuesday).

## pulse

Verified-anonymous sentiment polling, built on cryptographic blind
signatures. Employees answer honestly because the math — not a policy —
guarantees they can't be identified, even by the operator; only
sufficiently anonymous aggregates ever leave the system.

**What you get.** A k-anonymous sentiment aggregate on the same cadence
as the capacity report — the honest qualitative signal beside the
quantitative one, both feeding the next assessment. Details live in the
[pulse repo](https://github.com/como-technologies/pulse).
