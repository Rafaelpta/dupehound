//! Cross-language golden tests: for every supported language, a renamed
//! (type-2) clone pair must fingerprint identically, and the scan binary
//! must cluster them.

use std::process::Command;

struct Fixture {
    file_a: &'static str,
    file_b: &'static str,
    src_a: &'static str,
    src_b: &'static str,
    names: (&'static str, &'static str),
}
const PHP: Fixture = Fixture {
    file_a: "billing.php",
    file_b: "invoices.php",
    names: ("computeTotal", "calculateAmount"),
    src_a: r#"
<?php
function computeTotal($items, $taxRate) {
    $subtotal = 0.0;
    foreach ($items as $item) {
        $price = $item["price"] * $item["qty"];
        if (isset($item["discount"])) {
            $price = $price * (1.0 - $item["discount"]);
        }
        $subtotal += $price;
    }
    $tax = $subtotal * $taxRate;
    return round($subtotal + $tax, 2);
}
"#,
    src_b: r#"
<?php
function calculateAmount($rows, $vat) {
    $base = 0.0;
    foreach ($rows as $row) {
        $cost = $row["price"] * $row["qty"];
        if (isset($row["discount"])) {
            $cost = $cost * (1.0 - $row["discount"]);
        }
        $base += $cost;
    }
    $extra = $base * $vat;
    return round($base + $extra, 2);
}
"#,
};

const PYTHON: Fixture = Fixture {
    file_a: "billing.py",
    file_b: "invoices.py",
    names: ("compute_total", "calculate_amount"),
    src_a: r#"
def compute_total(items, tax_rate):
    subtotal = 0.0
    for item in items:
        price = item["price"] * item["qty"]
        if item.get("discount"):
            price = price * (1.0 - item["discount"])
        subtotal += price
    tax = subtotal * tax_rate
    return round(subtotal + tax, 2)
"#,
    src_b: r#"
def calculate_amount(rows, vat):
    base = 0.0
    for row in rows:
        cost = row["price"] * row["qty"]
        if row.get("discount"):
            cost = cost * (1.0 - row["discount"])
        base += cost
    extra = base * vat
    return round(base + extra, 2)
"#,
};

const GO: Fixture = Fixture {
    file_a: "server.go",
    file_b: "worker.go",
    names: ("retryRequest", "attemptCall"),
    src_a: r#"package main

import "time"

func retryRequest(url string, attempts int) (string, error) {
	var lastErr error
	for i := 0; i < attempts; i++ {
		body, err := fetch(url)
		if err == nil {
			return body, nil
		}
		lastErr = err
		time.Sleep(time.Duration(i*100) * time.Millisecond)
	}
	return "", lastErr
}
"#,
    src_b: r#"package main

import "time"

func attemptCall(endpoint string, retries int) (string, error) {
	var failure error
	for n := 0; n < retries; n++ {
		payload, err := fetch(endpoint)
		if err == nil {
			return payload, nil
		}
		failure = err
		time.Sleep(time.Duration(n*100) * time.Millisecond)
	}
	return "", failure
}
"#,
};

const JAVA: Fixture = Fixture {
    file_a: "OrderService.java",
    file_b: "CartService.java",
    names: ("findActive", "selectLive"),
    src_a: r#"
public class OrderService {
    public java.util.List<Order> findActive(java.util.List<Order> orders, int minAge) {
        java.util.List<Order> result = new java.util.ArrayList<>();
        for (Order order : orders) {
            if (order.getAge() >= minAge && order.isActive()) {
                result.add(order);
            } else if (order.isPriority()) {
                result.add(0, order);
            }
        }
        return result;
    }
}
"#,
    src_b: r#"
public class CartService {
    public java.util.List<Order> selectLive(java.util.List<Order> carts, int threshold) {
        java.util.List<Order> chosen = new java.util.ArrayList<>();
        for (Order cart : carts) {
            if (cart.getAge() >= threshold && cart.isActive()) {
                chosen.add(cart);
            } else if (cart.isPriority()) {
                chosen.add(0, cart);
            }
        }
        return chosen;
    }
}
"#,
};

const JAVASCRIPT: Fixture = Fixture {
    file_a: "auth.js",
    file_b: "session.js",
    names: ("validateToken", "checkCredential"),
    src_a: r#"
function validateToken(token, secret) {
    const parts = token.split(".");
    if (parts.length !== 3) {
        return { valid: false, reason: "malformed" };
    }
    const payload = JSON.parse(atob(parts[1]));
    if (payload.exp < Date.now() / 1000) {
        return { valid: false, reason: "expired" };
    }
    const signature = sign(parts[0] + "." + parts[1], secret);
    return { valid: signature === parts[2], reason: "checked" };
}
module.exports = { validateToken };
"#,
    src_b: r#"
function checkCredential(jwt, key) {
    const chunks = jwt.split(".");
    if (chunks.length !== 3) {
        return { valid: false, reason: "bad" };
    }
    const body = JSON.parse(atob(chunks[1]));
    if (body.exp < Date.now() / 1000) {
        return { valid: false, reason: "old" };
    }
    const sig = sign(chunks[0] + "." + chunks[1], key);
    return { valid: sig === chunks[2], reason: "done" };
}
module.exports = { checkCredential };
"#,
};
const RUBY: Fixture = Fixture {
    file_a: "auth.rb",
    file_b: "session.rb",
    names: ("validate_token", "check_credential"),
    src_a: r#"
def validate_token(token, secret)
    parts = token.split(".")
    if parts.length != 3
        return { valid: false, reason: "malformed" }
    end
    payload = JSON.parse(Base64.decode64(parts[1]))
    if payload["exp"] < Time.now.to_i
        return { valid: false, reason: "expired" }
    end
    signature = sign(parts[0] + "." + parts[1], secret)
    return { valid: signature == parts[2], reason: "checked" }
end
"#,
    src_b: r#"
def check_credential(jwt, key)
    chunks = jwt.split(".")
    if chunks.length != 3
        return { valid: false, reason: "bad" }
    end
    body = JSON.parse(Base64.decode64(chunks[1]))
    if body["exp"] < Time.now.to_i
        return { valid: false, reason: "old" }
    end
    sig = sign(chunks[0] + "." + chunks[1], key)
    return { valid: sig == chunks[2], reason: "done" }
end
"#,
};
const SWIFT: Fixture = Fixture {
    file_a: "server.swift",
    file_b: "worker.swift",
    names: ("retryRequest", "attemptCall"),
    src_a: r#"import Foundation

func retryRequest(url: String, attempts: Int) -> (String, String) {
    var lastErr = ""
    for i in 0..<attempts {
        let (body, err) = fetch(url)
        if err == "" {
            return (body, "")
        }
        lastErr = err
        sleep(i * 100)
    }
    return ("", lastErr)
}
"#,
    src_b: r#"import Foundation

func attemptCall(endpoint: String, retries: Int) -> (String, String) {
    var failure = ""
    for n in 0..<retries {
        let (payload, err) = fetch(endpoint)
        if err == "" {
            return (payload, "")
        }
        failure = err
        sleep(n * 100)
    }
    return ("", failure)
}
"#,
};

const C: Fixture = Fixture {
    file_a: "buffer.c",
    file_b: "stream.c",
    names: ("flush_data", "drain_bytes"),
    src_a: r#"
#include <stddef.h>

int flush_data(int *buf, size_t len) {
    int total = 0;
    for (size_t i = 0; i < len; i++) {
        int val = buf[i];
        if (val > 0) {
            total += val;
        } else {
            total -= val;
        }
    }
    return total;
}
"#,
    src_b: r#"
#include <stddef.h>

int drain_bytes(int *data, size_t count) {
    int sum = 0;
    for (size_t j = 0; j < count; j++) {
        int v = data[j];
        if (v > 0) {
            sum += v;
        } else {
            sum -= v;
        }
    }
    return sum;
}
"#,
};

const CPP: Fixture = Fixture {
    file_a: "renderer.cpp",
    file_b: "painter.cpp",
    names: ("draw_pixels", "paint_frames"),
    src_a: r#"
int draw_pixels(int x, int y, int w, int h) {
    int area = w * h;
    int offset = x + y;
    int result = area + offset;
    if (result > 0) {
        return result;
    } else {
        return -result;
    }
}
"#,
    src_b: r#"
int paint_frames(int u, int v, int r, int s) {
    int size = r * s;
    int start = u + v;
    int total = size + start;
    if (total > 0) {
        return total;
    } else {
        return -total;
    }
}
"#,
};
// Regression: functions returning pointers wrap the function_declarator in a
// pointer_declarator, which the first C query missed.
const C_POINTER_RETURN: Fixture = Fixture {
    file_a: "buf.c",
    file_b: "chunk.c",
    names: ("build_buffer", "make_chunk"),
    src_a: r#"
#include <stdlib.h>

char *build_buffer(int n, char fill) {
    char *p = malloc(n + 1);
    if (!p) {
        return NULL;
    }
    for (int i = 0; i < n; i++) {
        if (i % 2 == 0) {
            p[i] = fill;
        } else {
            p[i] = ' ';
        }
    }
    p[n] = 0;
    return p;
}
"#,
    src_b: r#"
#include <stdlib.h>

char *make_chunk(int size, char pad) {
    char *q = malloc(size + 1);
    if (!q) {
        return NULL;
    }
    for (int j = 0; j < size; j++) {
        if (j % 2 == 0) {
            q[j] = pad;
        } else {
            q[j] = ' ';
        }
    }
    q[size] = 0;
    return q;
}
"#,
};

// Regression: C++ methods defined outside the class have a qualified_identifier
// name, not an identifier, which the first cpp query missed.
const CPP_OUT_OF_LINE_METHOD: Fixture = Fixture {
    file_a: "engine.cpp",
    file_b: "worker.cpp",
    names: ("Engine::run", "Worker::exec"),
    src_a: r#"
int Engine::run(int a, int b, int c) {
    int acc = 0;
    for (int i = a; i < b; i++) {
        int v = i * c;
        if (v > 0) {
            acc += v;
        } else {
            acc -= v;
        }
    }
    return acc;
}
"#,
    src_b: r#"
int Worker::exec(int x, int y, int z) {
    int sum = 0;
    for (int k = x; k < y; k++) {
        int w = k * z;
        if (w > 0) {
            sum += w;
        } else {
            sum -= w;
        }
    }
    return sum;
}
"#,
};

const RUST: Fixture = Fixture {
    file_a: "parser.rs",
    file_b: "lexer.rs",
    names: ("split_records", "chunk_entries"),
    src_a: r#"
pub fn split_records(input: &str, sep: char) -> Vec<String> {
    let mut records = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == sep {
            records.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    records.push(current);
    records
}
"#,
    src_b: r#"
pub fn chunk_entries(text: &str, delim: char) -> Vec<String> {
    let mut entries = Vec::new();
    let mut buf = String::new();
    let mut quoted = false;
    for c in text.chars() {
        if quoted {
            buf.push(c);
            quoted = false;
        } else if c == '\\' {
            quoted = true;
        } else if c == delim {
            entries.push(std::mem::take(&mut buf));
        } else {
            buf.push(c);
        }
    }
    entries.push(buf);
    entries
}
"#,
};

fn run_scan_json(fixture: &Fixture) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(fixture.file_a), fixture.src_a).unwrap();
    std::fs::write(dir.path().join(fixture.file_b), fixture.src_b).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dupehound"))
        .args(["scan", "--json", "--min-tokens", "30"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn assert_clone_pair_detected(fixture: &Fixture) {
    let report = run_scan_json(fixture);
    let clusters = report["clusters"].as_array().unwrap();
    assert_eq!(
        clusters.len(),
        1,
        "{}: expected exactly one cluster, got {}",
        fixture.file_a,
        clusters.len()
    );
    let members = clusters[0]["members"].as_array().unwrap();
    let names: Vec<&str> = members
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&fixture.names.0),
        "missing {}",
        fixture.names.0
    );
    assert!(
        names.contains(&fixture.names.1),
        "missing {}",
        fixture.names.1
    );
    let similarity = clusters[0]["similarity"].as_f64().unwrap();
    assert!(
        similarity >= 0.95,
        "{}: type-2 clone should be ~identical, got {similarity}",
        fixture.file_a
    );
}

#[test]
fn python_renamed_clone_is_detected() {
    assert_clone_pair_detected(&PYTHON);
}

#[test]
fn go_renamed_clone_is_detected() {
    assert_clone_pair_detected(&GO);
}

#[test]
fn java_renamed_clone_is_detected() {
    assert_clone_pair_detected(&JAVA);
}
#[test]
fn swift_renamed_clone_is_detected() {
    assert_clone_pair_detected(&SWIFT);
}
#[test]
fn javascript_renamed_clone_is_detected() {
    assert_clone_pair_detected(&JAVASCRIPT);
}
#[test]
fn ruby_renamed_clone_is_detected() {
    assert_clone_pair_detected(&RUBY);
}

#[test]
fn rust_renamed_clone_is_detected() {
    assert_clone_pair_detected(&RUST);
}

#[test]
fn c_renamed_clone_is_detected() {
    assert_clone_pair_detected(&C);
}

#[test]
fn cpp_renamed_clone_is_detected() {
    assert_clone_pair_detected(&CPP);
}
#[test]
fn php_renamed_clone_is_detected() {
    assert_clone_pair_detected(&PHP);
}
#[test]
fn c_pointer_return_clone_is_detected() {
    assert_clone_pair_detected(&C_POINTER_RETURN);
}

#[test]
fn cpp_out_of_line_method_clone_is_detected() {
    assert_clone_pair_detected(&CPP_OUT_OF_LINE_METHOD);
}

#[test]
fn json_schema_is_versioned() {
    let report = run_scan_json(&PYTHON);
    assert_eq!(report["schema_version"], 1);
    assert!(report["score"]["slop_percent"].is_number());
    assert!(report["score"]["grade"].is_string());
}
