SHELL := /bin/bash
UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
SWIFT := xcrun --sdk macosx swift
PREFLIGHT := ./scripts/preflight-macos.sh
else
SWIFT := swift
PREFLIGHT := true
endif

.PHONY: build test app fixtures clean preflight

preflight:
	$(PREFLIGHT)

build: preflight
	cargo build --manifest-path runtime/Cargo.toml --release
	$(SWIFT) build --package-path app -c release --product DarwinPlay

test: preflight
	cargo test --manifest-path runtime/Cargo.toml
	$(SWIFT) test --package-path app

fixtures:
	./scripts/build-fixtures.sh

app:
	./scripts/build-macos.sh

clean:
	cargo clean --manifest-path runtime/Cargo.toml
	rm -rf app/.build fixtures/d3d11-triangle/build dist
