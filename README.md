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
  --calibrate             Benchmark key-generation and estimate search times
  --time                  Include total run time in search output
  --executor <local|cpu|gcp-gpu|aws-gpu>
                          Choose execution tier: local (free CPU), cpu (remote CPU),
                          gcp-gpu (GCP A100 GPU), aws-gpu (AWS GPU)
  --cpu-job <NAME>        Remote CPU batch job name (default: vanity-search-cpu)
  --cpu-queue <QUEUE>     Remote CPU batch queue (default: cpu-queue)
  --gcp-gpu-job <NAME>    GCP GPU job name (default: vanity-gpu-job)
  --gcp-gpu-image <IMG>   GCP GPU container image (default: gcr.io/myproj/vanity-gpu:latest)
  --aws-gpu-job <NAME>    AWS GPU batch job name (default: vanity-search-gpu)
  --aws-gpu-queue <QUEUE> AWS GPU batch queue (default: gpu-queue)
  --show-alphabet         Print the Base58 alphabet and exit
  -h, --help              Print help information
  -V, --version           Print version information

Examples:
```bash
# Search for address prefix "SOL" using 8 threads and mnemonic seed:
solana-vanity-seed --threads 8 --prefix SOL --words 12
# Same search, including total run time display:
solana-vanity-seed --threads 8 --prefix SOL --words 12 --time

# Search for address suffix "123" using raw keypairs:
solana-vanity-seed --suffix 123 --raw

 # Generate token mint address with prefix "TKN":
 solana-vanity-seed --threads 4 --token --prefix TKN
 # Run with different execution tiers:
 solana-vanity-seed --prefix ABC --executor local      # Local CPU (slowest, free)
 solana-vanity-seed --prefix ABC --executor cpu        # Remote CPU cluster (moderate speed, ~$0.10/hr)
 solana-vanity-seed --prefix ABC --executor gcp-gpu    # GCP A100 GPU (fast, cost-effective)
 solana-vanity-seed --prefix ABC --executor aws-gpu    # AWS GPU (fastest, higher cost)

# Custom infrastructure settings:
# Use a custom CPU queue and job name
solana-vanity-seed --prefix ABC --executor cpu \
  --cpu-job my-cpu-job --cpu-queue my-cpu-queue
# Use a custom GCP GPU job and image
solana-vanity-seed --prefix ABC --executor gcp-gpu \
  --gcp-gpu-job my-gpu-job --gcp-gpu-image gcr.io/myproj/custom-gpu:tag
 ```

## Address Mode Post-Search Steps

After running the address search command (e.g., `solana-vanity-seed --prefix SOL --words 12`), you'll see progress updates in the terminal. When a matching address is found, the tool will print:

- The generated public address
- The private key (for raw mode) or mnemonic phrase

Take these steps:

1. Copy and securely store your private key or mnemonic. Do not share it.
2. Import the private key or mnemonic into your Solana wallet (e.g., via `solana-keygen recover`, Phantom, etc.).
3. Fund your new address with SOL to cover transaction fees.
4. Use the address for your intended purpose (e.g., NFTs, DeFi, trading).

_Note: When implementing a Telegram bot, include these steps in the bot's `/help` command._

## Search Notifications

When running a search (via CLI or the interactive wizard), you will see live progress:
- 🔍 A start message indicating the search has begun.
- ❌ A progress line for each batch with no match, showing:
  - Batch number
  - Time taken for that batch
  - Total elapsed time
  Example:
    ❌ Batch #3: no match (batch: 2s, total: 6s)
- ⚡ Upon finding a match, the tool prints the address (and private key if applicable),
  the total run time, and a security reminder to record and delete the message.

## Interactive Wizard

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
