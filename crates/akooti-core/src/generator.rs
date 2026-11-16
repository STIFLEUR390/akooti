use std::fmt;
use std::sync::LazyLock;

use rand_core::{OsRng, RngCore};

use crate::error::VaultError;
use crate::models::SecretString;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";
const AMBIGUOUS: &[char] = &['0', 'O', '1', 'l', 'I'];
const EFF_WORDS: &str = include_str!("../data/eff_large_wordlist.txt");
const EFF_WORD_COUNT: usize = 7_776;

static WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    EFF_WORDS
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(_, word)| word))
        .collect()
});

/// Password generation settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordOptions {
    /// Requested character count.
    pub length: usize,
    /// Include lowercase ASCII letters.
    pub lowercase: bool,
    /// Include uppercase ASCII letters.
    pub uppercase: bool,
    /// Include decimal digits.
    pub digits: bool,
    /// Include punctuation.
    pub symbols: bool,
    /// Exclude `0`, `O`, `1`, `l`, and `I`.
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self {
            length: 24,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }
}

impl PasswordOptions {
    /// Enables every character class.
    pub fn all(length: usize) -> Self {
        Self {
            length,
            ..Self::default()
        }
    }
}

/// Diceware passphrase settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseOptions {
    /// Number of EFF words.
    pub words: usize,
    /// Text inserted between words.
    pub separator: String,
    /// Capitalize each word's first letter.
    pub capitalize: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        Self {
            words: 6,
            separator: "-".to_owned(),
            capitalize: true,
        }
    }
}

/// Generated secret and its estimated entropy.
pub struct GeneratedSecret {
    /// Generated value, zeroized on drop.
    pub value: SecretString,
    /// Estimated entropy in bits.
    pub entropy_bits: f64,
}

impl fmt::Debug for GeneratedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedSecret")
            .field("value", &"[REDACTED]")
            .field("entropy_bits", &self.entropy_bits)
            .finish()
    }
}

/// Generates a random password with independently configurable classes.
///
/// # Errors
///
/// Returns [`VaultError::InvalidInput`] for an invalid length or no character
/// classes.
pub fn generate_password(options: &PasswordOptions) -> Result<GeneratedSecret, VaultError> {
    if !(8..=256).contains(&options.length) {
        return Err(VaultError::InvalidInput(
            "password length must be between 8 and 256",
        ));
    }
    let selected = character_sets(options);
    if selected.is_empty() {
        return Err(VaultError::InvalidInput(
            "at least one password character set is required",
        ));
    }
    if options.length < selected.len() {
        return Err(VaultError::InvalidInput(
            "password is too short for the selected character sets",
        ));
    }

    let union = selected
        .iter()
        .flat_map(|set| set.chars())
        .collect::<Vec<_>>();
    let mut characters = Vec::with_capacity(options.length);
    for set in &selected {
        characters.push(random_character(set)?);
    }
    while characters.len() < options.length {
        characters.push(random_character_from(&union)?);
    }
    shuffle(&mut characters)?;

    let length = u32::try_from(options.length).map_err(|_| VaultError::Crypto)?;
    let alphabet = u32::try_from(union.len()).map_err(|_| VaultError::Crypto)?;
    let entropy_bits = f64::from(length) * f64::from(alphabet).log2();
    Ok(GeneratedSecret {
        value: SecretString::new(characters.into_iter().collect()),
        entropy_bits,
    })
}

/// Generates a passphrase from the embedded EFF long Diceware list.
///
/// # Errors
///
/// Returns [`VaultError::InvalidInput`] for an unsupported word count or
/// separator.
pub fn generate_passphrase(options: &PassphraseOptions) -> Result<GeneratedSecret, VaultError> {
    if !(3..=20).contains(&options.words) {
        return Err(VaultError::InvalidInput(
            "passphrase must contain between 3 and 20 words",
        ));
    }
    if options.separator.chars().count() > 4 || options.separator.chars().any(char::is_control) {
        return Err(VaultError::InvalidInput(
            "passphrase separator is invalid or too long",
        ));
    }

    let mut words = Vec::with_capacity(options.words);
    for _ in 0..options.words {
        let word_count = u32::try_from(EFF_WORD_COUNT).map_err(|_| VaultError::Crypto)?;
        let index = usize::try_from(random_below(word_count)?).map_err(|_| VaultError::Crypto)?;
        let word = WORDS.get(index).ok_or(VaultError::CorruptVault)?;
        words.push(if options.capitalize {
            capitalize(word)
        } else {
            (*word).to_owned()
        });
    }

    Ok(GeneratedSecret {
        value: SecretString::new(words.join(&options.separator)),
        entropy_bits: {
            let words = u32::try_from(options.words).map_err(|_| VaultError::Crypto)?;
            let word_count = u32::try_from(EFF_WORD_COUNT).map_err(|_| VaultError::Crypto)?;
            f64::from(words) * f64::from(word_count).log2()
        },
    })
}

fn character_sets(options: &PasswordOptions) -> Vec<String> {
    [
        (options.lowercase, LOWER),
        (options.uppercase, UPPER),
        (options.digits, DIGITS),
        (options.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, set)| {
        set.chars()
            .filter(|character| !options.exclude_ambiguous || !AMBIGUOUS.contains(character))
            .collect()
    })
    .collect()
}

fn random_character(characters: &str) -> Result<char, VaultError> {
    let characters = characters.chars().collect::<Vec<_>>();
    random_character_from(&characters)
}

fn random_character_from(characters: &[char]) -> Result<char, VaultError> {
    let upper = u32::try_from(characters.len()).map_err(|_| VaultError::Crypto)?;
    let index = random_below(upper)?;
    let index = usize::try_from(index).map_err(|_| VaultError::Crypto)?;
    characters.get(index).copied().ok_or(VaultError::Crypto)
}

fn random_below(upper: u32) -> Result<u32, VaultError> {
    if upper == 0 {
        return Err(VaultError::InvalidInput("random range is empty"));
    }
    let zone = u32::MAX - (u32::MAX % upper);
    loop {
        let random = OsRng.next_u32();
        if random < zone {
            return Ok(random % upper);
        }
    }
}

fn shuffle(characters: &mut [char]) -> Result<(), VaultError> {
    for index in (1..characters.len()).rev() {
        let upper = u32::try_from(index.saturating_add(1)).map_err(|_| VaultError::Crypto)?;
        let selected = usize::try_from(random_below(upper)?).map_err(|_| VaultError::Crypto)?;
        characters.swap(index, selected);
    }
    Ok(())
}

fn capitalize(word: &str) -> String {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
