//! Стилизованный вывод в консоль.
//!
//! Все сообщения проходят через этот модуль, чтобы обеспечить единый
//! визуальный стиль: информация — голубым, успех — зелёным, предупреждения
//! — жёлтым, ошибки — красным.

use console::style;

/// Заголовок сборки / секции.
pub fn header(title: &str) {
    println!();
    println!("{}", style(format!("── {} ──", title)).bold().cyan());
}

/// Обычное информационное сообщение.
pub fn info(msg: &str) {
    println!("{} {}", style("•").cyan().bold(), msg);
}

/// Успешное завершение шага.
pub fn success(msg: &str) {
    println!("{} {}", style("✓").green().bold(), msg);
}

/// Предупреждение (stderr).
pub fn warn(msg: &str) {
    eprintln!("{} {}", style("⚠").yellow().bold(), style(msg).yellow());
}

/// Ошибка (stderr).
pub fn error(msg: &str) {
    eprintln!("{} {}", style("✗").red().bold(), style(msg).red());
}

/// Алигненный вывод «Метка: значение».
pub fn field(label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    // Паддинг до стилизации — иначе ANSI-коды попадут в ширину.
    let padded = format!("{:<16}", format!("{}:", label));
    println!("  {} {}", style(padded).dim(), style(value).bold());
}
