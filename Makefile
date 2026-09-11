SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
MAKEFLAGS += --warn-undefined-variables --no-builtin-rules
.DEFAULT_GOAL := help

BIN      := imrule
BINARY   := target/release/$(BIN)
TEST_DIR := test-e2e
TMP      := /tmp/imrule-e2e
PREFIX   ?= $(HOME)/.local
ARGS     ?=

.PHONY: help setup fmt fmt-check lint test check build run clean install install-system uninstall coverage changelog deny test-e2e test-e2e-skills

help: ## 타깃 목록
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## 개발 도구 설치 (rustfmt, clippy) 및 의존성 받기
	rustup component add rustfmt clippy
	cargo fetch

fmt: ## 포맷 적용
	cargo fmt --all

fmt-check: ## 포맷 검사 (파일 수정 없음)
	cargo fmt --all --check

lint: ## clippy 린트 (경고를 에러로)
	cargo clippy --all-targets --all-features -- -D warnings

test: ## 통합·계약 테스트
	cargo test

check: fmt-check lint test ## CI 게이트: 포맷 검사 + 린트 + 테스트

build: ## 릴리스 빌드
	cargo build --release

run: ## 실행 (make run ARGS="--help")
	cargo run -- $(ARGS)

clean: ## 빌드 산출물 삭제
	cargo clean

install: build ## $(PREFIX)/bin에 설치 (기본 ~/.local/bin)
	@mkdir -p $(PREFIX)/bin
	cp $(BINARY) $(PREFIX)/bin/$(BIN)
	chmod +x $(PREFIX)/bin/$(BIN)
	@echo "imrule installed to $(PREFIX)/bin/$(BIN)"

install-system: PREFIX := /usr/local
install-system: install ## /usr/local/bin에 설치 (sudo 필요할 수 있음)

uninstall: ## $(PREFIX)/bin에서 제거
	rm -f $(PREFIX)/bin/$(BIN)

coverage: ## lcov 커버리지 리포트 생성 (cargo-llvm-cov)
	cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

changelog: ## 미배포 변경을 CHANGELOG.md에 추가 (git-cliff)
	git cliff --unreleased --prepend CHANGELOG.md

deny: ## 의존성 라이선스·보안 권고·출처 검사 (cargo-deny, deny.toml)
	cargo deny check

test-e2e: build ## 릴리스 바이너리로 셸 기반 E2E 테스트
	BINARY=$(BINARY) TMP=$(TMP) TEST_DIR=$(TEST_DIR) scripts/test-e2e.sh

test-e2e-skills: build ## 릴리스 바이너리로 skills E2E 테스트 (원격 GitHub 소스 포함)
	BINARY=$(BINARY) TMP=$(TMP) scripts/test-e2e-skills.sh
