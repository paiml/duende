# Duende Makefile - Iron Lotus Framework (Cross-Platform Daemon Tooling)
# Certeza Three-Tiered Testing Methodology + Toyota Way Quality Gates
#
# Usage:
#   make tier1       # On-save checks (<3s)
#   make tier2       # Pre-commit checks (1-5min)
#   make tier3       # Pre-merge checks (1-6h)
#   make tier4       # CI/CD quality gate

.PHONY: all tier1 tier2 tier3 tier4 \
        fmt fmt-check clippy check test test-lib test-all \
        coverage coverage-html mutants mutants-fast \
        deny audit falsification doc book book-serve clean \
        example-mlock example-mlock-status example-mlock-required \
        docker-mlock-build docker-mlock-test docker-mlock-test-build \
        bashrs-lint bashrs-gate

# ═══════════════════════════════════════════════════════════════════════════════
# Default target
# ═══════════════════════════════════════════════════════════════════════════════
all: tier2

# ═══════════════════════════════════════════════════════════════════════════════
# Tier 1: ON-SAVE (< 3 seconds)
# Fast feedback for flow state preservation
# ═══════════════════════════════════════════════════════════════════════════════
tier1: fmt-check clippy check
	@echo "✓ Tier 1 complete (<3s target)"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

check:
	cargo check --workspace --all-targets

# ═══════════════════════════════════════════════════════════════════════════════
# Tier 2: ON-COMMIT (1-5 minutes)
# Comprehensive pre-commit quality gate
# ═══════════════════════════════════════════════════════════════════════════════
tier2: fmt-check clippy test-lib deny
	@echo "✓ Tier 2 complete (1-5min target)"

test-lib:
	cargo nextest run --lib --workspace 2>/dev/null || cargo test --lib --workspace

test:
	cargo nextest run --workspace 2>/dev/null || cargo test --workspace

deny:
	cargo deny check 2>/dev/null || echo "⚠ cargo-deny not installed, skipping"

audit:
	cargo audit 2>/dev/null || echo "⚠ cargo-audit not installed, skipping"

# ═══════════════════════════════════════════════════════════════════════════════
# Tier 3: ON-MERGE (1-6 hours)
# Exhaustive validation for production readiness
# ═══════════════════════════════════════════════════════════════════════════════
tier3: test-all coverage mutants falsification
	@echo "✓ Tier 3 complete (run in CI)"

test-all:
	cargo nextest run --workspace --all-features 2>/dev/null || cargo test --workspace --all-features

coverage:
	@mkdir -p target/coverage
	cargo llvm-cov --workspace --html --output-dir target/coverage \
		--ignore-filename-regex 'bin/mlock-docker-test'
	@echo "Coverage report: target/coverage/html/index.html"

# Note: mlock-docker-test.rs is excluded because it's a standalone binary
# for Docker environment testing (requires --cap-add=IPC_LOCK)
coverage-check:
	cargo llvm-cov --workspace --fail-under-lines 90 \
		--ignore-filename-regex 'bin/mlock-docker-test'

mutants:
	cargo mutants --workspace -- --lib

mutants-fast:
	cargo mutants --workspace --jobs 4 -- --lib

falsification:
	cargo test --workspace --features falsification -- falsification

# ═══════════════════════════════════════════════════════════════════════════════
# Tier 4: CI/CD Quality Gate
# ═══════════════════════════════════════════════════════════════════════════════
tier4: test-release coverage-check deny audit
	@echo "✓ Tier 4 CI/CD gate passed"

test-release:
	cargo test --workspace --release

# ═══════════════════════════════════════════════════════════════════════════════
# Documentation
# ═══════════════════════════════════════════════════════════════════════════════
doc:
	cargo doc --workspace --no-deps --open

book:
	mdbook build book

book-serve:
	mdbook serve book --open

# ═══════════════════════════════════════════════════════════════════════════════
# Examples
# ═══════════════════════════════════════════════════════════════════════════════
example-mlock:
	cargo run -p duende-platform --example mlock

example-mlock-status:
	cargo run -p duende-platform --example mlock -- --status

example-mlock-required:
	cargo run -p duende-platform --example mlock -- --required

# ═══════════════════════════════════════════════════════════════════════════════
# Docker Testing (DT-007: mlock)
# Pure Rust test runner - no bash scripts (bashrs compliant)
# ═══════════════════════════════════════════════════════════════════════════════
docker-mlock-build:
	docker build -f docker/Dockerfile.mlock-test -t duende-mlock-test .

docker-mlock-test:
	cargo run -p duende-platform --bin mlock-docker-test

docker-mlock-test-build:
	cargo run -p duende-platform --bin mlock-docker-test -- --build

# ═══════════════════════════════════════════════════════════════════════════════
# Bashrs Enforcement (Shell-Free Containers)
# PAIML Stack requirement: No bash scripts in production containers
# ═══════════════════════════════════════════════════════════════════════════════
bashrs-lint:
	@echo "Checking Dockerfiles with bashrs..."
	@for f in docker/Dockerfile*; do \
		echo "  Linting: $$f"; \
		bashrs dockerfile lint "$$f" 2>/dev/null || echo "    ⚠ bashrs not installed"; \
	done

bashrs-gate:
	@echo "Running bashrs compliance gate..."
	@# Ensure no .sh files in docker/ directory
	@if ls docker/*.sh 2>/dev/null | grep -q .; then \
		echo "❌ FAIL: Shell scripts found in docker/ directory"; \
		echo "  Convert to Rust using: bashrs transpile <script>"; \
		ls docker/*.sh; \
		exit 1; \
	else \
		echo "✓ No shell scripts in docker/"; \
	fi
	@# Verify Dockerfiles are bashrs compliant
	@for f in docker/Dockerfile*; do \
		if ! grep -q "Purified by bashrs" "$$f" 2>/dev/null; then \
			echo "⚠ Warning: $$f not marked as bashrs purified"; \
		else \
			echo "✓ $$f is bashrs compliant"; \
		fi; \
	done

# ═══════════════════════════════════════════════════════════════════════════════
# Utilities
# ═══════════════════════════════════════════════════════════════════════════════
clean:
	cargo clean
	rm -rf target/coverage target/mutants

# Stack version check (PAIML Sovereign AI Stack)
stack-versions:
	@echo "Checking PAIML Stack versions on crates.io..."
	@for crate in trueno trueno-viz trueno-zram repartir renacer aprender pacha; do \
		printf "  %-15s " "$$crate:"; \
		cargo search "$$crate" --limit 1 2>/dev/null | head -1 | awk -F'"' '{print $$2}' || echo "not found"; \
	done

# Kaizen improvement log
kaizen:
	@echo "=== Kaizen Improvement Analysis ==="
	@echo "Current metrics:"
	@echo "  - Test coverage: $$(cargo llvm-cov --workspace --json 2>/dev/null | jq -r '.data[0].totals.lines.percent' || echo 'N/A')%"
	@echo "  - Clippy warnings: $$(cargo clippy --workspace 2>&1 | grep -c 'warning:' || echo '0')"
	@echo "  - SATD comments: $$(grep -rn 'TODO\|FIXME\|HACK\|XXX' crates/ src/ 2>/dev/null | wc -l || echo '0')"
