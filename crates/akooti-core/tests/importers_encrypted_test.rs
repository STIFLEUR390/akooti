mod common;

use akooti_core::{ImportError, ImportFormat, ImportOptions, SecretString, Vault};
use common::fixture;

const BUTTERCUP: &str = "b~>buttercup/b7vwB9JefVwz5fxgmdcx0J3PgbF7xs3oKroC11BuvqDrSJrGoR6lt6v+qQ3OJNzLFYeFrD6+nhWpHEOGJ8zXlxSdI/Gw++CAbX153sSJ69yqVtQJZDKe2XVNmHJqI+YCT1r0DRAZ06J8AiNRjjVYLDodPMjirLzZdOGNBXPIurSooZ6uQKZFHFT0UPVhRR5+XER13qY+Pg+mxT0mPRTZR1MiNGaAWdhBilo+nLbzTquTX3nxa2Ve0i3XX8kA9RjNW4uzWBhD3Gi59B5UFJ79tb6lr7CP2hEuGL6rcG7rjsmwwT4L+xZAsNJz5FKX6hLTRic6lk4Ov/M5ipSZnQSOJBIBoXJ642MAtqmmnRMwsnuRydFueyxUAXGIBL4MI6BB75tWQUMTZhPEIt68ubHmiV69/wJ0PRL0GQvEg3oIlbaHZ9ZHfwPJ7Edbk/CB1wYgiUx6TuVEKX5Ls4qIdO3ko1baGUW+NSbrlwM0K9Txon9URjs7sFrl3hJodx4CIEQmcvC3XMlfdxtgyb0U3ALY95Q==$00112233445566778899aabbccddeeff$buttercup-fixture-salt$e2e36d1b48ec3e14391851fc59e384fd4b0b9ebc00d92f5d1a3a2249c57738d7$5000$cbc";
const BITWARDEN: &str = r#"{"encrypted":true,"passwordProtected":true,"salt":"bitwarden-fixture-salt","kdfIterations":10000,"kdfType":0,"encKeyValidation_DO_NOT_EDIT":"2./+7dzLuqmYh3ZlVEMyIRAA==|JQTLCg5hQXuHujMyHpgPsg==|o5pViT8uXlRjVCfLMtnRmXJQ3Q5tboad0lwJNSbGYHc=","data":"2./+7dzLuqmYh3ZlVEMyIRAA==|ozMPwB8RgwIAh4P71u5Hke2uG6otQ70zFtYSccIZX8dMCG0FR4H0I4BJGwxJ9e4H+E6CB21kNYnlFiAAJOtJdlbByuauLgWNcWa2w3UjuZ1whmo9gVtZ4aDzai0NUsC9fIY0ysLFqiQkK8C4IFvbc75ZUKMWcprqxi7KfOFI872nw/5O/MN0smSu3tpXLDAexac2calyxRLByXa3wXu5HW0AHZ8g7pj5hT8Vs0WQmMoNYioC3UJbE79PWkFTZVPvvw2ylY7Xq13FQbhreYwDPBdCalMsroeSscJIAL3AG8t1rYU9b2GMxtkvz4NvkOHh6nKpIkQQZNENDwC9NSVFfaXPNMWd5Wux+oguwlnLarqi/EvG7/DbVpHaoIoJb+Hr/1Y2j+1o8hIjdl2CSPS1/FfCYAL6Z4ZtdGyLiC1rbKdvYMvdH7p2qCQU9DBiiKuQqzt287oSI2uc4c8jWsQ76sl+zAgn27OaPDHHvKfbU5clWtztADGvBiXtQBwweq2TQcucEijB19x5VHFqpJANBrvXxD/TkLO4WJrcft404U3KiYINqcwUZOm8XOJxpdKxVEGvVIjPsHsSr8U0yhcbsTY0RJ+HT/Bg7LctqKw4aT2ZcKPNG/VhXu6PTezs8rz0JinSRCdtUktUoG+3Z0jz2JrcO+nzBFHLGhP7SykVAOI=|+szChiRlGg9YM3S89/cHIBgSkZ38l9mEBi430uckH1M="}"#;
const PASSWORD: &str = "correct horse battery staple";

#[test]
fn buttercup_bcup_decrypts_and_rejects_wrong_password() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let password = SecretString::new(PASSWORD.to_owned());
    let report = vault
        .import_bytes(
            ImportFormat::Buttercup,
            BUTTERCUP.as_bytes(),
            Some(&password),
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 1);
    assert_eq!(report.routed, 1);
    let wrong = SecretString::new("wrong password".to_owned());
    let error = vault
        .import_bytes(
            ImportFormat::Buttercup,
            BUTTERCUP.as_bytes(),
            Some(&wrong),
            ImportOptions::default(),
        )
        .expect_err("wrong password");
    assert!(matches!(error, ImportError::InvalidPassword));
}

#[test]
fn bitwarden_password_protected_json_decrypts_and_rejects_wrong_password() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let password = SecretString::new(PASSWORD.to_owned());
    let report = vault
        .import_bytes(
            ImportFormat::BitwardenJson,
            BITWARDEN.as_bytes(),
            Some(&password),
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 1);
    let wrong = SecretString::new("wrong password".to_owned());
    let error = vault
        .import_bytes(
            ImportFormat::BitwardenJson,
            BITWARDEN.as_bytes(),
            Some(&wrong),
            ImportOptions::default(),
        )
        .expect_err("wrong password");
    assert!(matches!(error, ImportError::InvalidPassword));
}
