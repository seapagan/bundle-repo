use super::SecretScanError;
use super::partitions::{PartitionedScanners, RuleOrder};
use secrets_scanner::{Finding, ScanResult};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::PanicHookInfo;
use std::sync::Mutex;

static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());
type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;

pub(super) fn scan_sequential(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
) -> Result<ScanResult, SecretScanError> {
    let results = scanners
        .partitions
        .iter()
        .map(|partition| {
            (
                partition.ordinal,
                partition.scanner.scan_content_detailed(path, text),
            )
        })
        .collect();
    merge_results(scanners, results)
}

pub(super) fn scan_parallel(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
) -> Result<ScanResult, SecretScanError> {
    orchestrate_parallel(scanners, path, text, |partition, path, text| {
        Ok(vec![(
            partition.ordinal,
            partition.scanner.scan_content_detailed(path, text),
        )])
    })
}

fn orchestrate_parallel(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
    execute: impl Fn(
        &super::partitions::RulePartition,
        &str,
        &str,
    ) -> Result<Vec<(usize, ScanResult)>, SecretScanError>
    + Sync,
) -> Result<ScanResult, SecretScanError> {
    with_suppressed_worker_panic_output(|| {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(scanners.partitions.len());
            let mut spawn_failed = false;
            for partition in &scanners.partitions {
                let execute = &execute;
                match std::thread::Builder::new()
                    .spawn_scoped(scope, move || {
                        execute(partition, path, text)
                    }) {
                    Ok(handle) => handles.push(handle),
                    Err(_) => {
                        spawn_failed = true;
                        break;
                    }
                }
            }
            let mut results = Vec::with_capacity(handles.len());
            let mut worker_failed = false;
            let mut worker_panicked = false;
            for handle in handles {
                match handle.join() {
                    Ok(Ok(mut worker_results)) => {
                        results.append(&mut worker_results);
                    }
                    Ok(Err(_)) => worker_failed = true,
                    Err(_) => worker_panicked = true,
                }
            }
            if worker_panicked {
                return Err(SecretScanError::WorkerPanic);
            }
            if spawn_failed || worker_failed {
                return Err(SecretScanError::PartitionScanFailure);
            }
            merge_results(scanners, results)
        })
    })
}

fn with_suppressed_worker_panic_output<T>(operation: impl FnOnce() -> T) -> T {
    let _lock = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _hook = PanicHookGuard::install();
    operation()
}

struct PanicHookGuard {
    previous: Option<PanicHook>,
}

impl PanicHookGuard {
    fn install() -> Self {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(previous);
        }
    }
}

fn merge_results(
    scanners: &PartitionedScanners,
    results: Vec<(usize, ScanResult)>,
) -> Result<ScanResult, SecretScanError> {
    let mut seen_partitions = BTreeSet::new();
    let mut ordered = Vec::new();
    for (partition, result) in results {
        if result.findings_truncated {
            return Err(SecretScanError::TruncatedFindings);
        }
        if partition >= scanners.partitions.len()
            || !seen_partitions.insert(partition)
        {
            return Err(SecretScanError::PartitionIntegrity);
        }
        append_findings(scanners, partition, result.findings, &mut ordered)?;
    }
    if seen_partitions.len() != scanners.partitions.len() {
        return Err(SecretScanError::PartitionIntegrity);
    }
    ordered.sort_by_key(|(order, occurrence, _)| {
        (order.phase, order.source_index, *occurrence)
    });
    Ok(ScanResult {
        findings: ordered.into_iter().map(|(_, _, finding)| finding).collect(),
        findings_truncated: false,
    })
}

fn append_findings(
    scanners: &PartitionedScanners,
    partition: usize,
    findings: Vec<Finding>,
    ordered: &mut Vec<(RuleOrder, usize, Finding)>,
) -> Result<(), SecretScanError> {
    let mut occurrences = BTreeMap::<String, usize>::new();
    for finding in findings {
        let order = scanners
            .rule_order
            .get(&finding.rule_id)
            .copied()
            .ok_or(SecretScanError::PartitionIntegrity)?;
        if order.owner_partition != partition {
            return Err(SecretScanError::PartitionIntegrity);
        }
        let occurrence =
            occurrences.entry(finding.rule_id.clone()).or_default();
        ordered.push((order, *occurrence, finding));
        *occurrence += 1;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn merge_test_results(
    scanners: &PartitionedScanners,
    results: Vec<(usize, ScanResult)>,
) -> Result<ScanResult, SecretScanError> {
    merge_results(scanners, results)
}

#[cfg(test)]
pub(super) use test_support::{
    TestExecution, WorkerFault, scan_parallel_with_test_execution,
};

#[cfg(test)]
mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum WorkerFault {
        Error,
        Panic,
        Missing,
        Duplicate,
        Truncated,
    }

    pub(crate) struct TestExecution {
        pub(crate) fault: Option<(usize, WorkerFault)>,
        pub(crate) completion_order: Option<Vec<usize>>,
        pub(crate) completed: Arc<AtomicUsize>,
        pub(crate) observed_order: Arc<Mutex<Vec<usize>>>,
    }

    struct CompletionGuard(Arc<AtomicUsize>);

    impl Drop for CompletionGuard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct CompletionGate {
        order: Vec<usize>,
        next: Mutex<usize>,
        ready: Condvar,
        observed: Arc<Mutex<Vec<usize>>>,
    }

    impl CompletionGate {
        fn finish(&self, ordinal: usize) {
            let mut next = self
                .next
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while self.order.get(*next) != Some(&ordinal) {
                next = self
                    .ready
                    .wait(next)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            self.observed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(ordinal);
            *next += 1;
            self.ready.notify_all();
        }
    }

    pub(crate) fn scan_parallel_with_test_execution(
        scanners: &PartitionedScanners,
        path: &str,
        text: &str,
        execution: &TestExecution,
    ) -> Result<ScanResult, SecretScanError> {
        let gate = completion_gate(execution);
        orchestrate_parallel(scanners, path, text, |partition, path, text| {
            let _guard = CompletionGuard(Arc::clone(&execution.completed));
            let mut result =
                partition.scanner.scan_content_detailed(path, text);
            if let Some(gate) = &gate {
                gate.finish(partition.ordinal);
            }
            match execution
                .fault
                .filter(|fault| fault.0 == partition.ordinal)
                .map(|fault| fault.1)
            {
                Some(WorkerFault::Error) => {
                    Err(SecretScanError::PartitionScanFailure)
                }
                Some(WorkerFault::Panic) => {
                    panic!("private worker panic payload")
                }
                Some(WorkerFault::Missing) => Ok(Vec::new()),
                Some(WorkerFault::Duplicate) => Ok(vec![
                    (partition.ordinal, result.clone()),
                    (partition.ordinal, result),
                ]),
                Some(WorkerFault::Truncated) => {
                    result.findings_truncated = true;
                    Ok(vec![(partition.ordinal, result)])
                }
                None => Ok(vec![(partition.ordinal, result)]),
            }
        })
    }

    fn completion_gate(
        execution: &TestExecution,
    ) -> Option<Arc<CompletionGate>> {
        execution.completion_order.as_ref().map(|order| {
            Arc::new(CompletionGate {
                order: order.clone(),
                next: Mutex::new(0),
                ready: Condvar::new(),
                observed: Arc::clone(&execution.observed_order),
            })
        })
    }
}
