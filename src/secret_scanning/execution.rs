use super::SecretScanError;
use super::partitions::{PartitionedScanners, RuleOrder};
use secrets_scanner::{Finding, ScanResult};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::PanicHookInfo;
use std::sync::{Arc, Mutex};

static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());
type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;
type SharedPanicHook = Arc<Mutex<Option<PanicHook>>>;

thread_local! {
    static SCANNER_OPERATION: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn scan_sequential(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
) -> Result<ScanResult, SecretScanError> {
    orchestrate_sequential(scanners, path, text, |partition, path, text| {
        partition.scanner.scan_content_detailed(path, text)
    })
}

fn orchestrate_sequential(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
    execute: impl Fn(&super::partitions::RulePartition, &str, &str) -> ScanResult,
) -> Result<ScanResult, SecretScanError> {
    with_suppressed_scanner_panic_output(|| {
        orchestrate_sequential_protected(scanners, path, text, execute)
    })
}

fn orchestrate_sequential_protected(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
    execute: impl Fn(&super::partitions::RulePartition, &str, &str) -> ScanResult,
) -> Result<ScanResult, SecretScanError> {
    let mut results = Vec::with_capacity(scanners.partitions.len());
    for partition in &scanners.partitions {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _scanner = ScannerOperationGuard::enter();
                execute(partition, path, text)
            }))
            .map_err(|_| SecretScanError::ScanPanic)?;
        results.push((partition.ordinal, result));
    }
    merge_results(scanners, results)
}

pub(super) fn scan_parallel(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
) -> Result<ScanResult, SecretScanError> {
    orchestrate_parallel(
        scanners,
        path,
        text,
        |_| Ok(()),
        |partition, path, text| {
            vec![(
                partition.ordinal,
                partition.scanner.scan_content_detailed(path, text),
            )]
        },
    )
}

fn orchestrate_parallel(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
    before_spawn: impl Fn(usize) -> Result<(), ()>,
    execute: impl Fn(
        &super::partitions::RulePartition,
        &str,
        &str,
    ) -> Vec<(usize, ScanResult)>
    + Sync,
) -> Result<ScanResult, SecretScanError> {
    with_suppressed_scanner_panic_output(|| {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(scanners.partitions.len());
            let mut spawn_failed = false;
            for partition in &scanners.partitions {
                let execute = &execute;
                let handle = before_spawn(partition.ordinal).and_then(|()| {
                    std::thread::Builder::new()
                        .spawn_scoped(scope, move || {
                            let _scanner = ScannerOperationGuard::enter();
                            execute(partition, path, text)
                        })
                        .map_err(|_| ())
                });
                match handle {
                    Ok(handle) => handles.push(handle),
                    Err(_) => {
                        spawn_failed = true;
                        break;
                    }
                }
            }
            let mut results = Vec::with_capacity(handles.len());
            let mut worker_panicked = false;
            for handle in handles {
                match handle.join() {
                    Ok(mut worker_results) => {
                        results.append(&mut worker_results);
                    }
                    Err(_) => worker_panicked = true,
                }
            }
            if worker_panicked {
                return Err(SecretScanError::ScanPanic);
            }
            if spawn_failed {
                return Err(SecretScanError::PartitionScanFailure);
            }
            merge_results(scanners, results)
        })
    })
}

fn with_suppressed_scanner_panic_output<T>(
    operation: impl FnOnce() -> T,
) -> T {
    let _lock = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_suppressed_scanner_panic_output_locked(operation)
}

fn with_suppressed_scanner_panic_output_locked<T>(
    operation: impl FnOnce() -> T,
) -> T {
    let hook = PanicHookGuard::install();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    drop(hook);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

struct PanicHookGuard {
    previous: SharedPanicHook,
}

impl PanicHookGuard {
    fn install() -> Self {
        let previous = Arc::new(Mutex::new(Some(std::panic::take_hook())));
        let delegated = Arc::clone(&previous);
        std::panic::set_hook(Box::new(move |info| {
            let scanner_operation = SCANNER_OPERATION.with(Cell::get);
            if !scanner_operation {
                let previous = delegated
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(previous) = previous.as_ref() {
                    previous(info);
                }
            }
        }));
        Self { previous }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        let _installed = std::panic::take_hook();
        let previous = self
            .previous
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(previous) = previous {
            std::panic::set_hook(previous);
        }
    }
}

struct ScannerOperationGuard;

impl ScannerOperationGuard {
    fn enter() -> Self {
        SCANNER_OPERATION.with(|operation| operation.set(true));
        Self
    }
}

impl Drop for ScannerOperationGuard {
    fn drop(&mut self) {
        SCANNER_OPERATION.with(|operation| operation.set(false));
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
    TestExecution, WorkerFault, panic_hook_delivery_for_tests,
    panic_hook_restoration_for_tests, scan_parallel_with_test_execution,
    scan_sequential_with_test_panic,
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
        orchestrate_parallel(
            scanners,
            path,
            text,
            |ordinal| match execution.fault {
                Some((fault_ordinal, WorkerFault::Error))
                    if fault_ordinal == ordinal =>
                {
                    Err(())
                }
                _ => Ok(()),
            },
            |partition, path, text| {
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
                    Some(WorkerFault::Panic) => {
                        panic!("private scanner panic payload")
                    }
                    Some(WorkerFault::Missing) => Vec::new(),
                    Some(WorkerFault::Duplicate) => vec![
                        (partition.ordinal, result.clone()),
                        (partition.ordinal, result),
                    ],
                    Some(WorkerFault::Truncated) => {
                        result.findings_truncated = true;
                        vec![(partition.ordinal, result)]
                    }
                    Some(WorkerFault::Error) | None => {
                        vec![(partition.ordinal, result)]
                    }
                }
            },
        )
    }

    pub(crate) fn scan_sequential_with_test_panic(
        scanners: &PartitionedScanners,
        path: &str,
        text: &str,
    ) -> (Result<ScanResult, SecretScanError>, usize) {
        let _lock = PANIC_HOOK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let deliveries = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&deliveries);
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
        }));

        let result = with_suppressed_scanner_panic_output_locked(|| {
            orchestrate_sequential_protected(
                scanners,
                path,
                text,
                |_, _, _| panic!("private sequential scanner payload"),
            )
        });

        let restored = std::panic::take_hook();
        drop(restored);
        std::panic::set_hook(original);
        (result, deliveries.load(Ordering::SeqCst))
    }

    pub(crate) fn panic_hook_delivery_for_tests() -> usize {
        let _lock = PANIC_HOOK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let deliveries = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&deliveries);
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
        }));

        for _ in 0..8 {
            drop(PanicHookGuard::install());
        }
        {
            let _hook = PanicHookGuard::install();
            let worker = std::thread::spawn(|| {
                let _scanner = ScannerOperationGuard::enter();
                panic!("private scanner payload")
            });
            let unrelated =
                std::thread::spawn(|| panic!("unrelated panic payload"));
            assert!(worker.join().is_err());
            assert!(unrelated.join().is_err());
        }

        let installed = std::panic::take_hook();
        drop(installed);
        std::panic::set_hook(original);
        deliveries.load(Ordering::SeqCst)
    }

    pub(crate) fn panic_hook_restoration_for_tests() -> (bool, bool) {
        let _lock = PANIC_HOOK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let original = std::panic::take_hook();
        let marker = Arc::new(());
        let captured = Arc::clone(&marker);
        let replacement: PanicHook = Box::new(move |_| {
            let _ = &captured;
        });
        let expected =
            (&*replacement as *const dyn Fn(&PanicHookInfo<'_>)) as *const ();
        std::panic::set_hook(replacement);

        for _ in 0..8 {
            drop(PanicHookGuard::install());
        }
        let restored = std::panic::take_hook();
        let repeated = ((&*restored as *const dyn Fn(&PanicHookInfo<'_>))
            as *const ())
            == expected;
        std::panic::set_hook(restored);

        let unwind = std::panic::catch_unwind(|| {
            with_suppressed_scanner_panic_output_locked(|| {
                panic!("unrelated protected operation panic");
            });
        });
        assert!(unwind.is_err());
        let restored = std::panic::take_hook();
        let after_unwind = ((&*restored as *const dyn Fn(&PanicHookInfo<'_>))
            as *const ())
            == expected;
        drop(restored);
        std::panic::set_hook(original);
        (repeated, after_unwind)
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
