use bip39::{Language, Mnemonic};
use solana_sdk::signature::{Keypair, SeedDerivable, Signer};
use clap::Parser;
use rayon::prelude::*;
use bs58;
use rand::{thread_rng, RngCore};
use num_cpus;

// Define the Base58 alphabet for validation
const BASE58_ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

#[derive(Parser)]
#[clap(author, version, about = "Generate Solana vanity addresses interactively or via CLI")]
struct Args {
    /// Show the Base58 alphabet and exit
    #[clap(long)]
    show_alphabet: bool,
    /// Run in interactive wizard mode
    #[clap(long)]
    interactive: bool,
    /// Vanity prefix (Base58) to search for
    #[clap(long, value_parser)]
    prefix: Option<String>,
    /// Vanity suffix (Base58) to search for
    #[clap(long, value_parser)]
    suffix: Option<String>,
    /// Generate raw ED25519 keypairs (private key output)
    #[clap(long, conflicts_with = "token")]
    raw: bool,
    /// Generate a token address only (public key output)
    #[clap(long, help = "Generate a token address only (public key only)")]
    token: bool,
    /// Number of mnemonic words (12 or 24); only used if not --raw
    #[clap(long, default_value_t = 12, value_parser = parse_word_count)]
    words: usize,
    /// Number of CPU threads to use; defaults to all logical cores
    #[clap(long)]
    threads: Option<usize>,
}

fn parse_word_count(s: &str) -> Result<usize, String> {
    let count: usize = s.parse().map_err(|_| "Invalid number".to_string())?;
    if count == 12 || count == 24 {
        Ok(count)
    } else {
        Err("Words must be 12 or 24".to_string())
    }
}
// -- Interactive wizard support ------------------------------------------------
use std::io::{self, Write};
use std::time::Instant;

/// Product type for interactive mode: wallet address or token mint
#[derive(Debug)]
enum ProductType {
    Wallet,
    Token,
}

/// Whether to search by prefix or suffix
#[derive(Debug)]
enum SearchMode {
    Prefix(String),
    Suffix(String),
    /// Search for both a prefix and a suffix
    Both { prefix: String, suffix: String },
}

/// Generation type for interactive mode: raw keypair, mnemonic, or token address only
#[derive(Debug)]
enum GenerationMode {
    Raw,
    Mnemonic,
    Token,
}

/// Interactive wizard to collect options, estimate run time, and print the final command
fn interactive_mode() {
    println!("Welcome to the Solana Vanity Address Wizard!");
    // Product selection: Wallet or Token
    let product = loop {
        print!("What would you like to generate: Wallet (W) or Token mint address (T)? (default W): ");
        io::stdout().flush().unwrap();
        let mut choice = String::new();
        io::stdin().read_line(&mut choice).unwrap();
        match choice.trim().to_uppercase().as_str() {
            "" | "W" => break ProductType::Wallet,
            "T"      => break ProductType::Token,
            _         => println!("Please type W or T."),
        }
    };
    // If token mint, run token wizard
    if let ProductType::Token = product {
        // Token-specific wizard
        // Token name
        print!("Enter your token name (e.g. MyToken Coin): "); io::stdout().flush().unwrap();
        let mut token_name = String::new(); io::stdin().read_line(&mut token_name).unwrap();
        let token_name = token_name.trim();
        // Token ticker
        let token_ticker = loop {
            print!("Enter token ticker (e.g. TKN, uppercase letters only): "); io::stdout().flush().unwrap();
            let mut t = String::new(); io::stdin().read_line(&mut t).unwrap();
            let t = t.trim();
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase() && c.is_alphanumeric()) {
                break t.to_string();
            }
            println!("Invalid ticker. Use uppercase letters and digits only.");
        };
        // Threads selection
        let max_threads = num_cpus::get();
        let threads = loop {
            print!("How many threads to use for mint address search [1-{}] (default {}): ", max_threads, max_threads);
            io::stdout().flush().unwrap();
            let mut input = String::new(); io::stdin().read_line(&mut input).unwrap();
            let n = if input.trim().is_empty() {
                max_threads
            } else if let Ok(v) = input.trim().parse::<usize>() {
                v
            } else {
                println!("Please enter a number between 1 and {}.", max_threads);
                continue;
            };
            if n < 1 || n > max_threads {
                println!("Please enter a number between 1 and {}.", max_threads);
                continue;
            }
            if n > 10 {
                // Confirm for large thread counts
                let mut confirmed = false;
                loop {
                    print!("You chose {} threads, which may impact performance. Continue? (Y/N): ", n);
                    io::stdout().flush().unwrap();
                    let mut c = String::new(); io::stdin().read_line(&mut c).unwrap();
                    match c.trim().to_uppercase().as_str() {
                        "" | "Y" | "YES" => { confirmed = true; break; }
                        "N" | "NO"      => { println!("Let's choose again."); break; }
                        _                 => { println!("Please type Y or N."); continue; }
                    }
                }
                if !confirmed {
                    continue;
                }
            }
            break n;
        };
        // Search mode: prefix, suffix, or both
        let mode = loop {
            print!("Search mint address by Prefix (P), Suffix (S), or Both (B)? (default P): "); io::stdout().flush().unwrap();
            let mut c = String::new(); io::stdin().read_line(&mut c).unwrap();
            match c.trim().to_uppercase().as_str() {
                "" | "P" => break SearchMode::Prefix(prompt_pattern("prefix")),
                "S"      => break SearchMode::Suffix(prompt_pattern("suffix")),
                "B"      => {
                    let p = prompt_pattern("prefix");
                    let s = prompt_pattern("suffix");
                    break SearchMode::Both { prefix: p, suffix: s };
                }
                _         => println!("Please type P, S, or B."),
            }
        };
        // Calibration
        println!("\nCalibrating mint address generation speed...");
        let sample = 1_000;
        let start = Instant::now();
        for _ in 0..sample { generate_candidate(&mode, 0, true); }
        let elapsed = start.elapsed();
        let per_thread = sample as f64 / elapsed.as_secs_f64();
        let total_rate = per_thread * threads as f64;
        // Estimate
        let pat_len = match &mode {
            SearchMode::Prefix(p) => p.len(),
            SearchMode::Suffix(s) => s.len(),
            SearchMode::Both { prefix, suffix } => prefix.len() + suffix.len(),
        };
        let space = (BASE58_ALPHABET.len() as f64).powi(pat_len as i32);
        println!("\nEstimated total rate: {:.2} keys/sec", total_rate);
        println!("Search space: 58^{} ≈ {:.0} keys", pat_len, space);
        println!("Avg time: {}", format_duration(space / total_rate));
        // Final command
        println!("\nCopy & paste this command to start your token mint address search:");
        let mut cmd = format!("./target/release/solana-vanity-seed --threads {} --token ", threads);
        match &mode {
            SearchMode::Prefix(p) => cmd.push_str(&format!("--prefix {} ", p)),
            SearchMode::Suffix(s) => cmd.push_str(&format!("--suffix {} ", s)),
            SearchMode::Both { prefix, suffix } => cmd.push_str(&format!("--prefix {} --suffix {} ", prefix, suffix)),
        }
        println!("{}", cmd);
        // Post steps
        println!("\nPost-generation steps for your new token:");
        println!("1. Run the above command and note the 'Public Address' value as your token mint address.");
        println!("2. Create your SPL token:");
        println!("   spl-token create-token --decimals 9 <TOKEN_MINT_ADDRESS>");
        println!("3. Create an associated token account for yourself:");
        println!("   spl-token create-account <TOKEN_MINT_ADDRESS>");
        println!("4. Mint initial supply:");
        println!("   spl-token mint <TOKEN_MINT_ADDRESS> <AMOUNT>");
        println!("5. (Optional) Add liquidity on Raydium or Serum using the new token.");
        println!("   See https://docs.raydium.io/faqs/add-liquidity for guidance.");
        println!("\nToken Name: {}", token_name);
        println!("Token Ticker: ${}", token_ticker);
        return;
    }
    // --- Wallet wizard continues below ---
    // Threads selection with confirmation if above 10
    let max_threads = num_cpus::get();
    let threads = loop {
        print!("How many threads to use [1-{}] (default {}): ", max_threads, max_threads);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let t = input.trim();
        // Use default if blank, parse otherwise
        let n = if t.is_empty() {
            max_threads
        } else if let Ok(val) = t.parse::<usize>() {
            val
        } else {
            println!("Please enter a number between 1 and {}.", max_threads);
            continue;
        };
        // Validate range
        if n < 1 || n > max_threads {
            println!("Please enter a number between 1 and {}.", max_threads);
            continue;
        }
        // Confirm if using many threads
        if n > 10 {
            let mut confirmed = false;
            loop {
                print!("You chose {} threads, which may impact your system performance. Continue? (Y/N): ", n);
                io::stdout().flush().unwrap();
                let mut conf = String::new();
                io::stdin().read_line(&mut conf).unwrap();
                match conf.trim().to_uppercase().as_str() {
                    "" | "Y" | "YES" => { confirmed = true; break; }
                    "N" | "NO"      => { println!("Let's choose again."); break; }
                    _                 => { println!("Please type Y or N."); continue; }
                }
            }
            if !confirmed {
                continue;
            }
        }
        break n;
    };
    // Choose search mode: Prefix, Suffix, or Both
    let mode = loop {
        print!("Search by Prefix (P), Suffix (S), or Both (B)? (default P): ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let c = input.trim().to_uppercase();
        if c.is_empty() || c == "P" {
            break SearchMode::Prefix(prompt_pattern("prefix"));
        } else if c == "S" {
            break SearchMode::Suffix(prompt_pattern("suffix"));
        } else if c == "B" {
            let prefix = prompt_pattern("prefix");
            let suffix = prompt_pattern("suffix");
            break SearchMode::Both { prefix, suffix };
        }
        println!("Please type P, S, or B.");
    };
    // Choose generation type: raw keypair, mnemonic, or Cancel
    let gen_mode = loop {
        print!("Generate raw keypairs (R), derive from mnemonic (M), or Cancel (C)? (default M): ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let c = input.trim().to_uppercase();
        if c.is_empty() || c == "M" {
            break GenerationMode::Mnemonic;
        } else if c == "R" {
            break GenerationMode::Raw;
        } else if c == "C" {
            println!("Wizard cancelled.");
            return;
        }
        println!("Please type M, R, or C.");
    };
    // Words (only for mnemonic mode)
    let words = if let GenerationMode::Mnemonic = gen_mode {
        loop {
            print!("How many words for mnemonic? (12 or 24, default 12): ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let t = input.trim();
            if t.is_empty() {
                break 12;
            }
            if let Ok(n) = t.parse::<usize>() {
                if n == 12 || n == 24 {
                    break n;
                }
            }
            println!("Please enter 12 or 24.");
        }
    } else {
        0
    };
    // Calibration
    println!("\nCalibrating key generation speed (this may take a moment)...");
    let sample = 1_000;
    let start = Instant::now();
    // Treat token mode same as raw for calibration
    let raw_flag = matches!(gen_mode, GenerationMode::Raw | GenerationMode::Token);
    for _ in 0..sample {
        generate_candidate(&mode, words, raw_flag);
    }
    let elapsed = start.elapsed();
    let per_thread_rate = sample as f64 / elapsed.as_secs_f64();
    let total_rate = per_thread_rate * threads as f64;
    // Estimate search space
    let pattern_len = match &mode {
        SearchMode::Prefix(p) => p.len(),
        SearchMode::Suffix(s) => s.len(),
        SearchMode::Both { prefix, suffix } => prefix.len() + suffix.len(),
    };
    let avg_tries = (BASE58_ALPHABET.len() as f64).powi(pattern_len as i32);
    let avg_secs = avg_tries / total_rate;
    let best_secs = 1.0 / total_rate;
    let worst_secs = avg_secs * 5.0;
    println!("\nEstimated performance:");
    println!("  Key rate per thread: {:.2} keys/sec", per_thread_rate);
    println!("  Total rate ({} threads): {:.2} keys/sec", threads, total_rate);
    println!("  Search space: 58^{} ≈ {:.0} keys", pattern_len, avg_tries);
    println!("  Best-case (lucky first hit): {}", format_duration(best_secs));
    println!("  Average-case: {}", format_duration(avg_secs));
    println!("  Very likely (<5× avg): {}", format_duration(worst_secs));
    // Final command
    println!("\nCopy & paste this command to start searching:");
    let mut cmd = format!("./target/release/solana-vanity-seed --threads {} ", threads);
    // Generation mode flags
    match gen_mode {
        GenerationMode::Raw => cmd.push_str("--raw "),
        GenerationMode::Token => cmd.push_str("--token "),
        GenerationMode::Mnemonic => {},
    }
    // Search mode flags
    match &mode {
        SearchMode::Prefix(p) => cmd.push_str(&format!("--prefix {} ", p)),
        SearchMode::Suffix(s) => cmd.push_str(&format!("--suffix {} ", s)),
        SearchMode::Both { prefix, suffix } => cmd.push_str(&format!("--prefix {} --suffix {} ", prefix, suffix)),
    }
    // Mnemonic word count
    if let GenerationMode::Mnemonic = gen_mode {
        cmd.push_str(&format!("--words {}", words));
    }
    println!("{}", cmd);
}

/// Prompt the user to enter a prefix or suffix pattern
fn prompt_pattern(kind: &str) -> String {
    loop {
        print!("Enter {} (Base58 only): ", kind);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let p = input.trim();
        if !p.is_empty() && p.chars().all(|c| BASE58_ALPHABET.contains(c)) {
            return p.to_string();
        }
        println!("Invalid {}. Allowed Base58 characters: {}", kind, BASE58_ALPHABET);
    }
}

/// Generate a single candidate key (mnemonic or raw) for calibration
fn generate_candidate(_mode: &SearchMode, words: usize, raw: bool) {
    if raw {
        let _ = Keypair::new();
    } else {
        let entropy_bytes = if words == 12 { 16 } else { 32 };
        let mut rng = thread_rng();
        let mut entropy = vec![0u8; entropy_bytes];
        rng.fill_bytes(&mut entropy);
        let m = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
        let seed = m.to_seed("");
        let _ = Keypair::from_seed(&seed[..32]).unwrap();
    }
}

/// Format seconds into a human-readable string
fn format_duration(secs: f64) -> String {
    let s = secs.round() as u64;
    let days = s / 86_400;
    let hours = (s % 86_400) / 3_600;
    let mins = (s % 3_600) / 60;
    let secs = s % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if mins > 0 {
        parts.push(format!("{}m", mins));
    }
    parts.push(format!("{}s", secs));
    parts.join(" ")
}
// -- Search loop ---------------------------------------------------------------
/// Runs the brute-force search loop based on the given mode, word-count, and key generation mode
fn run_search(mode: SearchMode, words: usize, raw: bool, token: bool) {
    let batch_size = 1_000_000;
    // Track total and per-batch durations
    let total_start = Instant::now();
    let mut batch_count = 0;
    // Show start notification for wallet searches only
    if !token {
        println!("🔍 Starting address search...");
    }
    loop {
        batch_count += 1;
        let batch_start = Instant::now();
        let found = (0..batch_size).into_par_iter().find_map_any(|_| {
            if token {
                // Token address only: generate keypair, check prefix/suffix, return no mnemonic
                let keypair = Keypair::new();
                let pubkey = keypair.pubkey().to_string();
                if matches_mode(&mode, &pubkey) {
                    Some((String::new(), keypair))
                } else {
                    None
                }
            } else if raw {
                // Raw keypair: generate keypair, check, no mnemonic
                let keypair = Keypair::new();
                let pubkey = keypair.pubkey().to_string();
                if matches_mode(&mode, &pubkey) {
                    Some((String::new(), keypair))
                } else {
                    None
                }
            } else {
                // Mnemonic-derived keypair
                let entropy_bytes = if words == 12 { 16 } else { 32 };
                let mut rng = thread_rng();
                let mut entropy = vec![0u8; entropy_bytes];
                rng.fill_bytes(&mut entropy);
                let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
                let seed = mnemonic.to_seed("");
                let keypair = Keypair::from_seed(&seed[..32]).unwrap();
                let pubkey = keypair.pubkey().to_string();
                if matches_mode(&mode, &pubkey) {
                    Some((mnemonic.to_string(), keypair))
                } else {
                    None
                }
            }
        });
        if let Some((mnemonic, keypair)) = found {
            let pubkey = keypair.pubkey().to_string();
            let private_key = bs58::encode(&keypair.to_bytes()).into_string();
            let total_duration = total_start.elapsed();
            if token {
                println!("Token Address: {}", pubkey);
                println!("⏱ Total run time: {}", format_duration(total_duration.as_secs_f64()));
                println!("⚠️  Record your token address now, then delete this message for safety.");
            } else {
                if !raw {
                    println!("Mnemonic: {}", mnemonic);
                }
                println!("Public Address: {}", pubkey);
                println!("Base58 Private Key: {}", private_key);
                println!("⏱ Total run time: {}", format_duration(total_duration.as_secs_f64()));
                println!("⚠️  Record your address and private key now, then delete for safety.");
            }
            return;
        }
        let batch_duration = batch_start.elapsed();
        let total_duration = total_start.elapsed();
        // Batch progress notification for wallet searches only
        if !token {
            println!(
                "❌ Batch #{}: no match (batch: {}, total: {})",
                batch_count,
                format_duration(batch_duration.as_secs_f64()),
                format_duration(total_duration.as_secs_f64()),
            );
        }
    }
}

/// Checks whether a given public-key string matches the prefix/suffix mode and case rules
fn matches_mode(mode: &SearchMode, pubkey: &str) -> bool {
    match mode {
        SearchMode::Prefix(p) => {
            if !pubkey.starts_with(p) {
                return false;
            }
            // Next character rule after prefix
            match pubkey.chars().nth(p.len()) {
                Some(ch) if p.chars().all(|c| c.is_ascii_uppercase()) => ch.is_ascii_lowercase(),
                Some(ch) if p.chars().all(|c| c.is_ascii_lowercase()) => ch.is_ascii_uppercase(),
                Some(ch) if p.chars().all(|c| c.is_ascii_digit())    => ch.is_ascii_alphabetic(),
                Some(_)                                                => true,
                None                                                   => false,
            }
        }
        SearchMode::Suffix(s) => {
            if !pubkey.ends_with(s) {
                return false;
            }
            // Previous character rule before suffix
            let idx = pubkey.len().saturating_sub(s.len()).saturating_sub(1);
            match pubkey.chars().nth(idx) {
                Some(ch) if s.chars().all(|c| c.is_ascii_uppercase()) => ch.is_ascii_lowercase(),
                Some(ch) if s.chars().all(|c| c.is_ascii_lowercase()) => ch.is_ascii_uppercase(),
                Some(ch) if s.chars().all(|c| c.is_ascii_digit())    => ch.is_ascii_alphabetic(),
                Some(_)                                                => true,
                None                                                   => false,
            }
        }
        SearchMode::Both { prefix, suffix } => {
            // Combined prefix and suffix check
            // Prefix
            if !pubkey.starts_with(prefix) {
                return false;
            }
            let ok_prefix = match pubkey.chars().nth(prefix.len()) {
                Some(ch) if prefix.chars().all(|c| c.is_ascii_uppercase()) => ch.is_ascii_lowercase(),
                Some(ch) if prefix.chars().all(|c| c.is_ascii_lowercase()) => ch.is_ascii_uppercase(),
                Some(ch) if prefix.chars().all(|c| c.is_ascii_digit())    => ch.is_ascii_alphabetic(),
                Some(_)                                                    => true,
                None                                                       => false,
            };
            if !ok_prefix {
                return false;
            }
            // Suffix
            if !pubkey.ends_with(suffix) {
                return false;
            }
            let idx = pubkey.len().saturating_sub(suffix.len()).saturating_sub(1);
            let ok_suffix = match pubkey.chars().nth(idx) {
                Some(ch) if suffix.chars().all(|c| c.is_ascii_uppercase()) => ch.is_ascii_lowercase(),
                Some(ch) if suffix.chars().all(|c| c.is_ascii_lowercase()) => ch.is_ascii_uppercase(),
                Some(ch) if suffix.chars().all(|c| c.is_ascii_digit())    => ch.is_ascii_alphabetic(),
                Some(_)                                                    => true,
                None                                                       => false,
            };
            ok_suffix
        }
    }
}

fn main() {
    // Parse CLI and destructure to avoid partial moves
    let Args { show_alphabet, interactive, prefix, suffix, raw, token, words, threads: threads_opt } = Args::parse();
    // If requested, just show the Base58 alphabet and exit
    if show_alphabet {
        println!("Allowed Base58 alphabet: {}", BASE58_ALPHABET);
        return;
    }
    // If interactive mode, run the wizard and exit
    if interactive {
        interactive_mode();
        return;
    }
    // Determine search mode: prefix, suffix, or both
    let mode = match (prefix, suffix) {
        (Some(p), Some(s)) => SearchMode::Both { prefix: p, suffix: s },
        (Some(p), None)    => SearchMode::Prefix(p),
        (None, Some(s))    => SearchMode::Suffix(s),
        _ => {
            eprintln!("Error: must specify --prefix, --suffix, or both (or use --interactive)");
            return;
        }
    };
    // Validate patterns against Base58 alphabet
    let patterns = match &mode {
        SearchMode::Prefix(p)       => vec![p],
        SearchMode::Suffix(s)       => vec![s],
        SearchMode::Both { prefix, suffix } => vec![prefix, suffix],
    };
    for pat in patterns {
        for c in pat.chars() {
            if !BASE58_ALPHABET.contains(c) {
                eprintln!("Error: Invalid character '{}' in pattern", c);
                println!("Allowed Base58 alphabet: {}", BASE58_ALPHABET);
                return;
            }
        }
    }
    // Determine thread count (use all logical CPUs if not specified)
    let threads = threads_opt.unwrap_or_else(num_cpus::get);


    // Initialize the Rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("Failed to build thread pool");
    // Determine generation mode for non-interactive
    let gen_mode = if token {
        GenerationMode::Token
    } else if raw {
        GenerationMode::Raw
    } else {
        GenerationMode::Mnemonic
    };
    eprintln!("Starting search: {} threads, mode={:?}, gen_mode={:?}, words={}...", threads, mode, gen_mode, words);
    // Run the search loop
    run_search(mode, words, raw, token);
}
