/// Render a frontend diagnostic as a source-labelled Ariadne report.
/// Frontend errors use `line:column: message`; lexer columns count Unicode scalars.
pub fn render_diagnostic(file_name: &str, source: &str, message: &str) -> String {
    let mut parts = message.splitn(3, ':');
    let line_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let column_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let detail = parts.next().unwrap_or(message).trim();

    let line_start: usize = source
        .split_inclusive('\n')
        .take(line_number.saturating_sub(1))
        .map(|preceding| preceding.chars().count())
        .sum();
    let line_text = source.split('\n').nth(line_number - 1).unwrap_or("");
    let char_column = column_number - 1;
    let byte_column = line_text
        .char_indices()
        .nth(char_column)
        .map(|(index, _)| index)
        .unwrap_or(line_text.len());
    let start = line_start + char_column.min(line_text.chars().count());
    let remainder = line_text.get(byte_column..).unwrap_or("");
    let mut chars = remainder.chars();
    let first = chars.next();
    let token_len = match first {
        Some(character) if character == '_' || character.is_alphanumeric() => remainder
            .chars()
            .take_while(|character| *character == '_' || character.is_alphanumeric())
            .count(),
        Some(first) => {
            let second = chars.next();
            if matches!(
                (first, second),
                ('=', Some('='))
                    | ('!', Some('='))
                    | ('<', Some('='))
                    | ('>', Some('='))
                    | ('&', Some('&'))
                    | ('|', Some('|'))
                    | ('+', Some('='))
                    | ('-', Some('='))
            ) {
                2
            } else {
                1
            }
        }
        None => 0,
    };
    let end = start + token_len;
    let span = start..end;
    let report = Report::build(ReportKind::Error, (file_name, span.clone()))
        .with_message("compilation failed")
        .with_label(Label::new((file_name, span)).with_message(detail.to_string()))
        .finish();
    let mut output = Vec::new();
    if report
        .write((file_name, Source::from(source)), &mut output)
        .is_err()
    {
        return message.to_string();
    }
    String::from_utf8_lossy(&output).into_owned()
}
