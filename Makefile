# Build Solana BPF/SBF programs
.PHONY: build-sbf

build-sbf:
	cargo build-sbf --manifest-path program/Cargo.toml
