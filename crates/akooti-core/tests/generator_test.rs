use akooti_core::{PassphraseOptions, PasswordOptions, generate_passphrase, generate_password};

#[test]
fn password_generation_honors_length_classes_and_ambiguity() {
    let options = PasswordOptions {
        length: 48,
        symbols: true,
        exclude_ambiguous: true,
        ..PasswordOptions::default()
    };
    let generated = generate_password(&options).expect("password");
    let value = generated.value.expose();

    assert_eq!(value.chars().count(), 48);
    assert!(
        value
            .chars()
            .any(|character| character.is_ascii_lowercase())
    );
    assert!(
        value
            .chars()
            .any(|character| character.is_ascii_uppercase())
    );
    assert!(value.chars().any(|character| character.is_ascii_digit()));
    assert!(
        value
            .chars()
            .any(|character| !character.is_ascii_alphanumeric())
    );
    assert!(!value.chars().any(|character| "0O1lI".contains(character)));
    assert!(generated.entropy_bits > 150.0);

    let second = generate_password(&options).expect("second password");
    assert_ne!(value, second.value.expose());
}

#[test]
fn passphrase_generation_honors_eff_words_and_case() {
    let options = PassphraseOptions {
        words: 6,
        separator: "::".to_owned(),
        capitalize: true,
    };
    let generated = generate_passphrase(&options).expect("passphrase");
    let parts = generated.value.expose().split("::").collect::<Vec<_>>();

    assert_eq!(parts.len(), 6);
    // Four EFF entries are hyphenated (`t-shirt`, `yo-yo`, ...), so the words are
    // not purely alphabetic even though the list is the expected one.
    assert!(parts.iter().all(|word| {
        word.chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
            && word
                .chars()
                .all(|character| character.is_ascii_alphabetic() || character == '-')
    }));
    assert!(generated.entropy_bits > 77.0);
    assert!(generated.entropy_bits < 78.0);
}
