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
    /// Vanity prefix (Base58) to search for (cannot be used with --suffix)
    #[clap(long, value_parser, conflicts_with = "suffix")]
    prefix: Option<String>,
    /// Vanity suffix (Base58) to search for (cannot be used with --prefix)
    #[clap(long, value_parser, conflicts_with = "prefix")]
    suffix: Option<String>,
    /// Generate raw ED25519 keypairs instead of deriving from mnemonic
    #[clap(long)]
    raw: bool,
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

/// Whether to search by prefix or suffix
#[derive(Debug)]
enum SearchMode {
    Prefix(String),
    Suffix(String),
}

/// Interactive wizard to collect options, estimate run time, and print the final command
fn interactive_mode() {
    println!("Welcome to the Solana Vanity Address Wizard!");
    // Threads
    let max_threads = num_cpus::get();
    let threads = loop {
        print!("How many threads to use [1-{}] (default {}): ", max_threads, max_threads);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let t = input.trim();
        if t.is_empty() {
            break max_threads;
        }
        if let Ok(n) = t.parse::<usize>() {
            if n >= 1 && n <= max_threads {
                break n;
            }
        }
        println!("Please enter a number between 1 and {}.", max_threads);
    };
    // Prefix or Suffix
    let mode = loop {
        print!("Search by Prefix (P) or Suffix (S)? (default P): ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let c = input.trim().to_uppercase();
        if c.is_empty() || c == "P" {
            break SearchMode::Prefix(prompt_pattern("prefix"));
        } else if c == "S" {
            break SearchMode::Suffix(prompt_pattern("suffix"));
        }
        println!("Please type P or S.");
    };
    // Mode: raw or mnemonic
    let raw = loop {
        print!("Generate raw keypairs (R) or derive from mnemonic (M)? (default M): ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let c = input.trim().to_uppercase();
        if c.is_empty() || c == "M" {
            break false;
        } else if c == "R" {
            break true;
        }
        println!("Please type M or R.");
    };
    // Words (if mnemonic)
    let words = if raw {
        0
    } else {
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
    };
    // Calibration
    println!("\nCalibrating key generation speed (this may take a moment)...");
    let sample = 10_000;
    let start = Instant::now();
    for _ in 0..sample {
        generate_candidate(&mode, words, raw);
    }
    let elapsed = start.elapsed();
    let per_thread_rate = sample as f64 / elapsed.as_secs_f64();
    let total_rate = per_thread_rate * threads as f64;
    // Estimate search space
    let pattern_len = match &mode {
        SearchMode::Prefix(p) | SearchMode::Suffix(p) => p.len(),
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
    let mut cmd = format!("solana-vanity-seed --threads {} ", threads);
    if raw {
        cmd.push_str("--raw ");
    }
    match &mode {
        SearchMode::Prefix(p) => cmd.push_str(&format!("--prefix {} ", p)),
        SearchMode::Suffix(s) => cmd.push_str(&format!("--suffix {} ", s)),
    }
    if !raw {
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
fn generate_candidate(mode: &SearchMode, words: usize, raw: bool) {
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
/// Runs the brute-force search loop based on the given mode, word-count, and thread settings
fn run_search(mode: SearchMode, words: usize, threads: usize, raw: bool) {
    let batch_size = 1_000_000;
    loop {
        let found = (0..batch_size).into_par_iter().find_map_any(|_| {
            if raw {
                let keypair = Keypair::new();
                let pubkey = keypair.pubkey().to_string();
                if matches_mode(&mode, &pubkey) {
                    Some((String::new(), keypair))
                } else {
                    None
                }
            } else {
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
            if !raw {
                println!("Mnemonic: {}", mnemonic);
            }
            let pubkey = keypair.pubkey().to_string();
            let private_key = bs58::encode(&keypair.to_bytes()).into_string();
            println!("Public Address: {}", pubkey);
            println!("Base58 Private Key: {}", private_key);
            break;
        }
        println!("No match found in batch, continuing...");
    }
}

/// Checks whether a given public-key string matches the prefix/suffix mode and case rules
fn matches_mode(mode: &SearchMode, pubkey: &str) -> bool {
    match mode {
        SearchMode::Prefix(p) => {
            if !pubkey.starts_with(p) {
                return false;
            }
            // Next character rule
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
            // Previous character rule
            let idx = pubkey.len().saturating_sub(s.len()).saturating_sub(1);
            match pubkey.chars().nth(idx) {
                Some(ch) if s.chars().all(|c| c.is_ascii_uppercase()) => ch.is_ascii_lowercase(),
                Some(ch) if s.chars().all(|c| c.is_ascii_lowercase()) => ch.is_ascii_uppercase(),
                Some(ch) if s.chars().all(|c| c.is_ascii_digit())    => ch.is_ascii_alphabetic(),
                Some(_)                                                => true,
                None                                                   => false,
            }
        }
    }
}

fn main() {
    // Parse CLI and destructure to avoid partial moves
    let Args { show_alphabet, interactive, prefix, suffix, raw, words, threads: threads_opt } = Args::parse();
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
    // Determine search mode: prefix or suffix
    let mode = if let Some(p) = prefix {
        SearchMode::Prefix(p)
    } else if let Some(s) = suffix {
        SearchMode::Suffix(s)
    } else {
        eprintln!("Error: must specify --prefix or --suffix (or use --interactive)");
        return;
    };
    // Determine thread count (use all logical CPUs if not specified)
    let threads = threads_opt.unwrap_or_else(num_cpus::get);
    // Validate pattern against Base58 alphabet
    let pattern = match &mode {
        SearchMode::Prefix(p) | SearchMode::Suffix(p) => p,
    };
    for c in pattern.chars() {
        if !BASE58_ALPHABET.contains(c) {
            eprintln!("Error: Invalid character '{}' in pattern", c);
            println!("Allowed Base58 alphabet: {}", BASE58_ALPHABET);
            return;
        }
    }
    // Determine thread count (use all logical CPUs if not specified)
    let threads = threads_opt.unwrap_or_else(num_cpus::get);


    // Initialize the Rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("Failed to build thread pool");
    eprintln!("Starting search: {} threads, mode={:?}, words={}, raw={}...", threads, mode, words, raw);
    // Run the search loop (prefix or suffix, mnemonic or raw)
    run_search(mode, words, threads, raw);
}
