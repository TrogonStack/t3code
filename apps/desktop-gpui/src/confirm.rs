//! How a confirmation message becomes a dialog (`confirmDialog.ts`, `ConfirmDialogHost.tsx`):
//! the title is the first line that ends in a question mark, the rest is the description.

/// `resolveConfirmDialogCopy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    pub title: String,
    pub description: Option<String>,
}

pub fn split_message(message: &str) -> Copy {
    let normalized = message.trim();
    let lines: Vec<&str> = normalized.split('\n').collect();
    if let Some(at) = lines.iter().position(|line| line.trim().ends_with('?')) {
        let description = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != at)
            .map(|(_, line)| *line)
            .collect::<Vec<_>>()
            .join("\n");
        let description = description.trim();
        return Copy {
            title: lines[at].trim().to_owned(),
            description: (!description.is_empty()).then(|| description.to_owned()),
        };
    }
    if let Some(mark) = normalized.find('?') {
        let (title, rest) = normalized.split_at(mark + 1);
        let rest = rest.trim();
        return Copy {
            title: title.trim().to_owned(),
            description: (!rest.is_empty()).then(|| rest.to_owned()),
        };
    }
    Copy {
        title: "Confirm action".to_owned(),
        description: Some(if normalized.is_empty() {
            "This action requires your confirmation.".to_owned()
        } else {
            normalized.to_owned()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(title: &str, description: Option<&str>) -> Copy {
        Copy {
            title: title.to_owned(),
            description: description.map(str::to_owned),
        }
    }

    #[test]
    fn the_question_line_is_the_title_and_the_other_lines_the_description() {
        assert_eq!(
            split_message("Delete \"a\"?\nThis cannot be undone."),
            copy("Delete \"a\"?", Some("This cannot be undone."))
        );
        assert_eq!(
            split_message("Path: /x\nRemove it?\nGone for good."),
            copy("Remove it?", Some("Path: /x\nGone for good."))
        );
        assert_eq!(
            split_message("  Archive thread?  "),
            copy("Archive thread?", None)
        );
    }

    #[test]
    fn a_question_mark_inside_a_line_splits_after_it() {
        assert_eq!(
            split_message("Really? Yes, really."),
            copy("Really?", Some("Yes, really."))
        );
    }

    #[test]
    fn a_message_without_a_question_is_the_description() {
        assert_eq!(
            split_message("Heads up"),
            copy("Confirm action", Some("Heads up"))
        );
        assert_eq!(
            split_message("  "),
            copy(
                "Confirm action",
                Some("This action requires your confirmation.")
            )
        );
    }
}
