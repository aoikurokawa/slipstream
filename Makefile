# Build Solana BPF/SBF programs
.PHONY: build-sbf build-idl

build-sbf:
	cargo build-sbf --manifest-path program/Cargo.toml

build-idl:
	jito-shank-cli \
        --program-env-path ./config/program.env \
        --output-idl-path ./program/idl/ \
        generate \
        --program-id-key "SLIPSTREAM_PROGRAM_ID" \
        --idl-name slipstream \
        --module-paths "program"
