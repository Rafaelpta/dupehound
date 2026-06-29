"""Cost estimation and a hard spend cap for the paid agent tier.

The agent arm costs real money and its cost grows with repo size. To avoid
surprises we (1) estimate the full run from a cheap probe on the smallest slice,
print the projection, and (2) enforce a hard cumulative cap, skipping any run
that would push spend over the ceiling. Per-run caps are also passed to the CLI
via --max-budget-usd.
"""


class BudgetTracker:
    def __init__(self, cap_usd):
        self.cap = cap_usd
        self._spent = 0.0
        self.skipped = 0

    def spent(self):
        return self._spent

    def remaining(self):
        return max(0.0, self.cap - self._spent)

    def record(self, usd):
        if usd:
            self._spent += usd

    def can_afford(self, est_usd):
        return (self._spent + max(0.0, est_usd)) <= self.cap


def project(probe_usd, probe_loc, slice_locs, models, n):
    """Linear-in-LOC projection of total spend across slices x models x n runs.

    probe_usd is the measured cost of one agent run on a slice of probe_loc.
    Returns (per_slice_estimate, grand_total).
    """
    rate = (probe_usd / probe_loc) if probe_loc else 0.0
    per_slice = {loc: rate * loc for loc in slice_locs}
    grand_total = sum(per_slice.values()) * len(models) * n
    return per_slice, grand_total


def fmt_projection(per_slice, grand_total, models, n, cap):
    lines = ["Projected agent spend (linear-in-LOC from probe):"]
    for loc in sorted(per_slice):
        lines.append("  ~$%.3f per run at %d LOC" % (per_slice[loc], loc))
    lines.append("  x %d models x %d runs  =>  ~$%.2f total" % (len(models), n, grand_total))
    lines.append("  budget cap: $%.2f  (%s)" % (
        cap, "within cap" if grand_total <= cap else "OVER cap -- runs will stop at the ceiling"))
    return "\n".join(lines)
