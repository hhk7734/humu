# Setting SHELL to bash allows bash commands to be executed by recipes.
# Options are set to exit when a recipe line exits non-zero or a piped command fails.
SHELL = /usr/bin/env bash -o pipefail
.SHELLFLAGS = -ec

.PHONY: all
all: build

##@ General

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Development

.PHONY: fmt
fmt: ## Run cargo fmt against code.
	cargo fmt

.PHONY: fmt-check
fmt-check: ## Check code formatting.
	cargo fmt --check

.PHONY: lint
lint: ## Run clippy linter.
	cargo clippy -- -D warnings

.PHONY: test
test: fmt-check lint ## Run tests.
	cargo test

##@ Build

.PHONY: build
build: fmt lint ## Build binary.
	cargo build --release

.PHONY: install
install: ## Install binary to cargo bin path.
	cargo install --path .

.PHONY: run
run: ## Run from your host.
	cargo run -- $(ARGS)
