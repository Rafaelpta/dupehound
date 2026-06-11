#!/usr/bin/env bash
# Builds the acme-api demo repository used for README screenshots: five
# months of distinct human-written modules, then seven months where renamed
# near-copies pile up (the "agent era"). Deterministic on purpose.
set -euo pipefail

DEST="${1:-/tmp/acme-api}"
rm -rf "$DEST"
mkdir -p "$DEST"/src/{auth,billing,api,utils,jobs,store,notify}
cd "$DEST"
git init -q -b main
git config user.email demo@acme.dev
git config user.name "acme dev"

commit_month() {
  GIT_AUTHOR_DATE="$1T10:30:00" GIT_COMMITTER_DATE="$1T10:30:00" git add -A
  GIT_AUTHOR_DATE="$1T10:30:00" GIT_COMMITTER_DATE="$1T10:30:00" git commit -qm "$2"
}

# ---------------- 2025-01 ----------------
cat > src/utils/date.ts <<'EOF'
export function formatDate(input: Date, locale: string): string {
    const year = input.getFullYear();
    const month = String(input.getMonth() + 1).padStart(2, "0");
    const day = String(input.getDate()).padStart(2, "0");
    if (locale === "us") {
        return month + "/" + day + "/" + year;
    }
    if (locale === "iso") {
        return year + "-" + month + "-" + day;
    }
    const hours = String(input.getHours()).padStart(2, "0");
    const minutes = String(input.getMinutes()).padStart(2, "0");
    return day + "/" + month + "/" + year + " " + hours + ":" + minutes;
}
EOF
cat > src/store/cache.ts <<'EOF'
interface Entry<V> { value: V; expiresAt: number; hits: number }

export class TtlCache<V> {
    private map = new Map<string, Entry<V>>();
    constructor(private maxEntries: number, private ttlMs: number) {}

    get(key: string): V | undefined {
        const entry = this.map.get(key);
        if (!entry) {
            return undefined;
        }
        if (entry.expiresAt < Date.now()) {
            this.map.delete(key);
            return undefined;
        }
        entry.hits += 1;
        return entry.value;
    }

    set(key: string, value: V): void {
        if (this.map.size >= this.maxEntries && !this.map.has(key)) {
            let coldest: string | undefined;
            let fewest = Infinity;
            for (const [k, e] of this.map) {
                if (e.hits < fewest) {
                    fewest = e.hits;
                    coldest = k;
                }
            }
            if (coldest !== undefined) {
                this.map.delete(coldest);
            }
        }
        this.map.set(key, { value, expiresAt: Date.now() + this.ttlMs, hits: 0 });
    }

    purgeExpired(): number {
        const now = Date.now();
        let removed = 0;
        for (const [k, e] of this.map) {
            if (e.expiresAt < now) {
                this.map.delete(k);
                removed += 1;
            }
        }
        return removed;
    }
}
EOF
commit_month 2025-01-12 "date utils and ttl cache"

# ---------------- 2025-02 ----------------
cat > src/auth/token.ts <<'EOF'
export function verifyToken(token: string, secret: string): { ok: boolean; sub?: string } {
    const parts = token.split(".");
    if (parts.length !== 3) {
        return { ok: false };
    }
    const payload = JSON.parse(Buffer.from(parts[1], "base64url").toString());
    if (typeof payload.exp !== "number" || payload.exp < Date.now() / 1000) {
        return { ok: false };
    }
    const expected = sign(parts[0] + "." + parts[1], secret);
    return expected === parts[2] ? { ok: true, sub: payload.sub } : { ok: false };
}

function sign(data: string, secret: string): string {
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
        hash = ((hash << 5) - hash + data.charCodeAt(i) * secret.length) | 0;
    }
    return hash.toString(16);
}
EOF
cat > src/auth/password.ts <<'EOF'
const MIN_LENGTH = 12;

export function passwordIssues(candidate: string): string[] {
    const issues: string[] = [];
    if (candidate.length < MIN_LENGTH) {
        issues.push(`must be at least ${MIN_LENGTH} characters`);
    }
    if (!/[a-z]/.test(candidate) || !/[A-Z]/.test(candidate)) {
        issues.push("must mix upper and lower case");
    }
    if (!/\d/.test(candidate)) {
        issues.push("must contain a digit");
    }
    const sequences = ["1234", "abcd", "qwer", "0000"];
    for (const seq of sequences) {
        if (candidate.toLowerCase().includes(seq)) {
            issues.push("contains a guessable sequence");
            break;
        }
    }
    return issues;
}

export function strengthScore(candidate: string): number {
    let pool = 0;
    if (/[a-z]/.test(candidate)) pool += 26;
    if (/[A-Z]/.test(candidate)) pool += 26;
    if (/\d/.test(candidate)) pool += 10;
    if (/[^a-zA-Z0-9]/.test(candidate)) pool += 32;
    const entropy = candidate.length * Math.log2(Math.max(pool, 1));
    return Math.min(100, Math.round(entropy));
}
EOF
commit_month 2025-02-09 "auth: tokens and password rules"

# ---------------- 2025-03 ----------------
cat > src/billing/invoice.ts <<'EOF'
export function computeInvoiceTotal(items: LineItem[], taxRate: number): number {
    let subtotal = 0;
    for (const item of items) {
        let line = item.unitPrice * item.quantity;
        if (item.discountPct) {
            line *= 1 - item.discountPct / 100;
        }
        subtotal += line;
    }
    const tax = subtotal * taxRate;
    return Math.round((subtotal + tax) * 100) / 100;
}

interface LineItem { unitPrice: number; quantity: number; discountPct?: number }
EOF
cat > src/billing/dunning.ts <<'EOF'
type Stage = "reminder" | "warning" | "suspension" | "writeoff";

export function nextDunningStage(daysOverdue: number, amountDue: number): Stage | null {
    if (daysOverdue <= 0 || amountDue <= 0) {
        return null;
    }
    if (daysOverdue < 7) {
        return "reminder";
    }
    if (daysOverdue < 21) {
        return amountDue > 500 ? "warning" : "reminder";
    }
    if (daysOverdue < 60) {
        return "suspension";
    }
    return amountDue < 25 ? "writeoff" : "suspension";
}

export function dunningEmailSubject(stage: Stage, invoiceNo: string): string {
    switch (stage) {
        case "reminder":
            return `Friendly reminder: invoice ${invoiceNo}`;
        case "warning":
            return `Action needed on invoice ${invoiceNo}`;
        case "suspension":
            return `Service suspension notice (${invoiceNo})`;
        case "writeoff":
            return `Invoice ${invoiceNo} closed`;
    }
}
EOF
commit_month 2025-03-14 "billing: invoices and dunning"

# ---------------- 2025-04 ----------------
cat > src/api/pagination.ts <<'EOF'
export function paginate<T>(rows: T[], page: number, perPage: number) {
    const total = rows.length;
    const pages = Math.max(1, Math.ceil(total / perPage));
    const current = Math.min(Math.max(1, page), pages);
    const start = (current - 1) * perPage;
    return {
        data: rows.slice(start, start + perPage),
        meta: { total, pages, current, perPage },
    };
}
EOF
cat > src/api/ratelimit.ts <<'EOF'
interface Bucket { tokens: number; updatedAt: number }

export class RateLimiter {
    private buckets = new Map<string, Bucket>();
    constructor(private capacity: number, private refillPerSec: number) {}

    allow(clientId: string, cost = 1): boolean {
        const now = Date.now();
        let bucket = this.buckets.get(clientId);
        if (!bucket) {
            bucket = { tokens: this.capacity, updatedAt: now };
            this.buckets.set(clientId, bucket);
        }
        const elapsed = (now - bucket.updatedAt) / 1000;
        bucket.tokens = Math.min(this.capacity, bucket.tokens + elapsed * this.refillPerSec);
        bucket.updatedAt = now;
        if (bucket.tokens < cost) {
            return false;
        }
        bucket.tokens -= cost;
        return true;
    }

    retryAfterSeconds(clientId: string, cost = 1): number {
        const bucket = this.buckets.get(clientId);
        if (!bucket || bucket.tokens >= cost) {
            return 0;
        }
        return Math.ceil((cost - bucket.tokens) / this.refillPerSec);
    }
}
EOF
commit_month 2025-04-11 "api: pagination and rate limiting"

# ---------------- 2025-05 ----------------
cat > src/jobs/retry.ts <<'EOF'
export async function withRetry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {
    let lastError: unknown;
    for (let i = 0; i < attempts; i++) {
        try {
            return await fn();
        } catch (err) {
            lastError = err;
            const backoff = Math.min(1000 * 2 ** i, 30000);
            await new Promise((resolve) => setTimeout(resolve, backoff));
        }
    }
    throw lastError;
}
EOF
cat > src/notify/digest.ts <<'EOF'
interface Event { kind: string; actor: string; at: number }

export function buildDigest(events: Event[], maxItems: number): string[] {
    const byKind = new Map<string, Event[]>();
    for (const event of events) {
        const list = byKind.get(event.kind) ?? [];
        list.push(event);
        byKind.set(event.kind, list);
    }
    const lines: string[] = [];
    const kinds = [...byKind.keys()].sort(
        (a, b) => byKind.get(b)!.length - byKind.get(a)!.length,
    );
    for (const kind of kinds) {
        const group = byKind.get(kind)!;
        const actors = [...new Set(group.map((e) => e.actor))];
        const sample = actors.slice(0, 3).join(", ");
        const others = actors.length > 3 ? ` and ${actors.length - 3} others` : "";
        lines.push(`${group.length}× ${kind} by ${sample}${others}`);
        if (lines.length >= maxItems) {
            break;
        }
    }
    return lines;
}
EOF
commit_month 2025-05-16 "jobs: retry; notify: digest"

# ------------- agent era: renamed near-copies -------------
dup() { sed -e "$2" "src/$1" > "src/$3"; }

dup utils/date.ts 's/formatDate/renderTimestamp/; s/\binput\b/value/g; s/locale/style/g; s/"us"/"en"/; s/"iso"/"sortable"/' api/timestamps.ts
commit_month 2025-06-13 "timestamp rendering for api responses"

dup billing/invoice.ts 's/computeInvoiceTotal/calculateOrderAmount/; s/items/entries/g; s/taxRate/vatRate/g; s/item/entry/g; s/subtotal/base/g; s/LineItem/OrderEntry/g' api/orders.ts
commit_month 2025-07-18 "order amount calculation"

dup auth/token.ts 's/verifyToken/checkSessionToken/; s/token/sessionJwt/g; s/secret/signingKey/g; s/parts/segments/g; s/payload/claims/g; s/expected/computed/g' api/session.ts
commit_month 2025-08-15 "session validation for websocket endpoints"

dup utils/date.ts 's/formatDate/stringifyDate/; s/\binput\b/d/g; s/locale/region/g; s/"us"/"mdy"/; s/"iso"/"ymd"/' jobs/report_dates.ts
commit_month 2025-09-12 "scheduled report dates"

dup jobs/retry.ts 's/withRetry/retryOperation/; s/attempts/maxTries/g; s/lastError/failure/g; s/backoff/delay/g; s/\bfn\b/operation/g' billing/retry_payment.ts
dup api/pagination.ts 's/paginate/pageResults/; s/rows/records/g; s/perPage/pageSize/g; s/current/active/g; s/start/offset/g' billing/page_invoices.ts
commit_month 2025-10-17 "payment retries and invoice paging"

dup billing/invoice.ts 's/computeInvoiceTotal/sumCartTotal/; s/items/cartLines/g; s/taxRate/taxPct/g; s/item/cartLine/g; s/subtotal/running/g; s/LineItem/CartLine/g' api/cart.ts
commit_month 2025-11-14 "cart totals"

dup auth/token.ts 's/verifyToken/validateApiKey/; s/token/apiKey/g; s/secret/appSecret/g; s/parts/pieces/g; s/payload/body/g; s/expected/calc/g' jobs/apikey.ts
dup utils/date.ts 's/formatDate/humanizeDate/; s/\binput\b/when/g; s/locale/fmt/g' billing/dates.ts
commit_month 2025-12-12 "api key validation, billing dates"

echo "demo repo ready at $DEST"
