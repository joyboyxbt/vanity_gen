# Solana Vanity Seed Generator

Generate Solana wallet vanity addresses or token mint addresses via CLI or an interactive wizard.

## Features
  - Generate ED25519 keypairs or derive from BIP-39 mnemonics (12 or 24 words).
  - Vanity search by prefix, suffix, or both in public Solana addresses.
  - Interactive wizard mode with simple prompts and validation (for non-technical users).
  - Token mint address search and post-deployment walkthrough (create SPL token, mint, add liquidity).
  - Thread parallelism with safe defaults and confirmation for high thread counts.
  - Runtime estimation (best-case, average-case, worst-case) based on key generation rates.
  - Show Base58 alphabet for reference or error feedback.

## Installation
Ensure you have Rust and Cargo installed (https://rustup.rs).

```bash
# Clone repository
git clone https://github.com/joyboyxbt/vanity_gen.git
cd vanity_gen

# Build release binary
cargo build --release

# Or run directly
cargo run -- --help
```

## CLI Usage
```
solana-vanity-seed [OPTIONS] --prefix <PREFIX> [--suffix <SUFFIX>]
```

Options:
  --prefix <PREFIX>       Vanity prefix (Base58) to search for
  --suffix <SUFFIX>       Vanity suffix (Base58) to search for
  --token                 Generate a token mint address only (public key output)
  --raw                   Generate raw keypair (no mnemonic, private key output)
  --words <12|24>         Number of words if deriving from mnemonic (default 12)
  --threads <N>           Number of CPU threads to use (default = all logical cores)
  --interactive           Run interactive wizard mode
  --show-alphabet         Print the Base58 alphabet and exit
  -h, --help              Print help information
  -V, --version           Print version information

Examples:
```bash
# Search for address prefix "SOL" using 8 threads and mnemonic seed:
solana-vanity-seed --threads 8 --prefix SOL --words 12

# Search for address suffix "123" using raw keypairs:
solana-vanity-seed --suffix 123 --raw

# Generate token mint address with prefix "TKN":
solana-vanity-seed --threads 4 --token --prefix TKN
```

## Interactive Wizard
Run the wizard for guided prompts and final copy–paste command:
```bash
cargo run -- --interactive
```

### Wallet Mode
1. Choose **Wallet (W)** or **Token (T)**.
2. For wallet:
   - Select thread count (max = logical cores, confirmation above 10).
   - Choose search by Prefix (P), Suffix (S), or Both (B).
   - Choose generation mode: Raw keypair (R) or Mnemonic (M).
   - If mnemonic: choose 12 or 24 words.
   - Calibrate performance and show runtime estimates.
   - Copy & paste the generated `solana-vanity-seed` command.

### Token Mode
1. Enter **Token Name** (e.g. "USA Coin").
2. Enter **Token Ticker** (uppercase alphanumeric, e.g. "USA").
3. Select thread count and confirm if >10.
4. Choose search by Prefix (P), Suffix (S), or Both (B).
5. Calibrate and show performance estimates.
6. Copy & paste the `solana-vanity-seed --token ...` command.
7. **Post-deployment steps**:
   ```bash
   # Create the SPL token (9 decimals):
   spl-token create-token --decimals 9 <TOKEN_MINT_ADDRESS>

   # Create your associated token account:
   spl-token create-account <TOKEN_MINT_ADDRESS>

   # Mint initial supply:
   spl-token mint <TOKEN_MINT_ADDRESS> <AMOUNT>

   # (Optional) Add liquidity via Raydium or Serum:
   # https://docs.raydium.io/faqs/add-liquidity
   ```

## Base58 Alphabet
If you ever need to reference valid characters:
```bash
cargo run -- --show-alphabet
# or with built binary
./target/release/solana-vanity-seed --show-alphabet
```
Output:
```
Allowed Base58 alphabet: 123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz
```
