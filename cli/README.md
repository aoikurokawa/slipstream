# Slipstream CLI

## Command

### List All

```bash
spl-stake-pool list-all
```

### Stake Pool List

```bash
spl-stake-pool list Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb -v
```

### Help

```bash
cargo r -p slipstream-cli -- --help
```

### Swap

```bash
cargo r -p slipstream-cli -- \
    swap \
    --amount-in 1000000000 \
    --min-out 900000000 \
    --rpc-url  https://api.mainnet-beta.solana.com \
    --pool-a Hr9pzexrBge3vgmBNRR8u42CNQgBXdHm4UkUN2DH4a7r \
    --pool-b Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb \
    --validator-vote 3N7s9zXMZ4QqvHQR15t5GNHyqc89KduzMP7423eWiD5g
```
