use super::{
    PATH_COMPONENT_SCAN_PATH, RepositoryPathScan, ScanSchedule,
    SecretScanError, SecretScanner, SkipReason, SkippedItemKind,
    SkippedRepositoryItem,
};

struct PendingSkipped {
    original_prefix: String,
    item: SkippedRepositoryItem,
}

impl SecretScanner {
    pub(crate) fn scan_repository_paths(
        &self,
        paths: Vec<String>,
    ) -> Result<RepositoryPathScan, SecretScanError> {
        let mut included = Vec::with_capacity(paths.len());
        let mut skipped = Vec::new();
        let mut findings = 0;

        for path in paths {
            if covered_by_subtree(&path, &skipped) {
                continue;
            }
            let components = path.split('/').collect::<Vec<_>>();
            let mut safe_components = Vec::with_capacity(components.len());
            let mut affected = None;
            for (index, component) in components.iter().enumerate() {
                let redaction = self.redact(
                    PATH_COMPONENT_SCAN_PATH,
                    component,
                    ScanSchedule::RepositoryPath,
                )?;
                findings += redaction.findings;
                safe_components.push(redaction.text);
                if redaction.findings > 0 {
                    affected = Some((index, redaction.secret_type));
                    break;
                }
            }

            let Some((index, secret_type)) = affected else {
                included.push(path);
                continue;
            };
            let kind = if index + 1 == components.len() {
                SkippedItemKind::File
            } else {
                SkippedItemKind::Subtree
            };
            let pending = PendingSkipped {
                original_prefix: components[..=index].join("/"),
                item: SkippedRepositoryItem {
                    kind,
                    safe_path: safe_components.join("/"),
                    reason: SkipReason::SecretInPath { secret_type },
                },
            };
            record_skipped(&mut skipped, pending);
        }

        let mut skipped = skipped
            .into_iter()
            .map(|pending| pending.item)
            .collect::<Vec<_>>();
        skipped.sort_by(|left, right| {
            skipped_sort_key(left).cmp(&skipped_sort_key(right))
        });
        Ok(RepositoryPathScan {
            included,
            skipped,
            findings,
        })
    }
}

fn covered_by_subtree(path: &str, skipped: &[PendingSkipped]) -> bool {
    skipped.iter().any(|pending| {
        pending.item.kind == SkippedItemKind::Subtree
            && path_is_at_or_below(path, &pending.original_prefix)
    })
}

fn record_skipped(skipped: &mut Vec<PendingSkipped>, pending: PendingSkipped) {
    if pending.item.kind == SkippedItemKind::Subtree {
        skipped.retain(|existing| {
            !path_is_at_or_below(
                &existing.original_prefix,
                &pending.original_prefix,
            )
        });
    }
    skipped.push(pending);
}

fn path_is_at_or_below(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn skipped_sort_key(
    item: &SkippedRepositoryItem,
) -> (&str, &str, Option<&str>) {
    let secret_type = match &item.reason {
        SkipReason::SecretInPath { secret_type } => secret_type.as_deref(),
    };
    (item.safe_path.as_str(), item.kind.as_str(), secret_type)
}
