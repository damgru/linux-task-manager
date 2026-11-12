use crate::{model::Process, Sort};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Name,
    Type,
    Status,
    Cpu,
    TotalCpu,
    Memory,
    TotalMemory,
    Disk,
    Pid,
    Parent,
    User,
    Command,
    Threads,
    Priority,
    Read,
    Write,
    Network,
    Gpu,
    GpuEngine,
    Power,
}
impl Column {
    pub fn all() -> [Self; 20] {
        use Column::*;
        [
            Name,
            Type,
            Status,
            Cpu,
            TotalCpu,
            Memory,
            TotalMemory,
            Disk,
            Pid,
            Parent,
            User,
            Command,
            Threads,
            Priority,
            Read,
            Write,
            Network,
            Gpu,
            GpuEngine,
            Power,
        ]
    }
    pub fn defaults() -> Vec<Self> {
        use Column::*;
        vec![Name, Type, Status, Cpu, TotalCpu, Memory, TotalMemory, Disk, Pid]
    }
    pub fn details() -> Vec<Self> {
        use Column::*;
        vec![Name, Pid, Status, User, Cpu, Memory, Threads, Priority]
    }
    pub fn parse_list(value: &str) -> Vec<Self> {
        let mut columns = vec![];
        for key in value.split(',') {
            if let Some(column) = Self::all().into_iter().find(|c| c.key() == key) {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
        }
        if !columns.contains(&Self::Name) {
            columns.insert(0, Self::Name);
        }
        columns
    }
    pub fn key(self) -> &'static str {
        use Column::*;
        match self {
            Name => "Name",
            Type => "Type",
            Status => "Status",
            Cpu => "CPU",
            TotalCpu => "Total CPU",
            Memory => "Memory",
            TotalMemory => "Total memory",
            Disk => "Disk",
            Pid => "PID",
            Parent => "Parent PID",
            User => "User name",
            Command => "Command line",
            Threads => "Threads",
            Priority => "Priority",
            Read => "Disk read",
            Write => "Disk written",
            Network => "Network",
            Gpu => "GPU",
            GpuEngine => "GPU engine",
            Power => "Power usage",
        }
    }
    pub fn width(self) -> f32 {
        match self {
            Self::Name => 280.0,
            Self::Type => 170.0,
            Self::Command => 360.0,
            Self::Status | Self::User => 135.0,
            Self::Parent => 105.0,
            Self::Cpu | Self::Pid | Self::Threads => 85.0,
            _ => 115.0,
        }
    }
    pub fn sort(self) -> Option<Sort> {
        use Column::*;
        Some(match self {
            Name => Sort::Name,
            Cpu => Sort::Cpu,
            TotalCpu => Sort::TotalCpu,
            Memory => Sort::Memory,
            TotalMemory => Sort::TotalMemory,
            Disk => Sort::Disk,
            Pid => Sort::Pid,
            Parent => Sort::Parent,
            User => Sort::User,
            Command => Sort::Command,
            Threads => Sort::Threads,
            Priority => Sort::Priority,
            Read => Sort::Read,
            Write => Sort::Write,
            Status => Sort::Status,
            _ => return None,
        })
    }
    pub fn numeric(self) -> bool {
        matches!(
            self,
            Self::Cpu
                | Self::TotalCpu
                | Self::Memory
                | Self::TotalMemory
                | Self::Disk
                | Self::Pid
                | Self::Parent
                | Self::Threads
                | Self::Priority
                | Self::Read
                | Self::Write
        )
    }
}
fn matching_in_group(included: &HashSet<u32>, searching: bool) -> HashSet<u32> {
    if searching {
        included.clone()
    } else {
        HashSet::new()
    }
}
/// Preserve sibling sorting, include ancestors of search matches, and guard
/// against cycles and missing parents in a procfs snapshot taken during churn.
pub fn tree_rows(
    processes: &[Process],
    order: &[usize],
    matches: &HashSet<u32>,
    folded: &HashSet<(u32, u64)>,
    searching: bool,
) -> Vec<(usize, usize, bool)> {
    let by_pid: HashMap<_, _> = processes.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    let allowed: HashSet<_> = order.iter().map(|&i| processes[i].pid).collect();
    let mut included: HashSet<_> = matches.intersection(&allowed).copied().collect();
    for &pid in matching_in_group(&included, searching).iter() {
        let mut next = pid;
        let mut path = HashSet::new();
        while let Some(&i) = by_pid.get(&next) {
            if !path.insert(next) {
                break;
            }
            next = processes[i].parent;
            if !allowed.contains(&next) {
                break;
            }
            if by_pid.contains_key(&next) {
                included.insert(next);
            }
        }
    }
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for &i in order {
        let p = &processes[i];
        if !included.contains(&p.pid) {
            continue;
        }
        if p.parent == p.pid || !included.contains(&p.parent) {
            roots.push(i);
        } else {
            children.entry(p.parent).or_default().push(i);
        }
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    // The second pass also visits orphan cycles. Folded descendants are marked
    // visited so they cannot reappear as additional roots.
    for root in roots.into_iter().chain(order.iter().copied()) {
        let mut stack = vec![(root, 0, true)];
        while let Some((i, depth, visible)) = stack.pop() {
            let p = &processes[i];
            if !included.contains(&p.pid) || !seen.insert(p.pid) {
                continue;
            }
            let kids = children.get(&p.pid);
            if visible {
                result.push((i, depth, kids.is_some_and(|v| !v.is_empty())));
            }
            let show = visible && (searching || !folded.contains(&(p.pid, p.start)));
            if let Some(kids) = kids {
                stack.extend(kids.iter().rev().map(|&i| (i, depth + 1, show)));
            }
        }
    }
    result
}
pub fn move_column(columns: &mut Vec<Column>, source: Column, target: Column) {
    if let (Some(from), Some(to)) = (columns.iter().position(|c| *c == source), columns.iter().position(|c| *c == target)) {
        let column = columns.remove(from);
        columns.insert(to, column);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    Section(bool, usize),
    Process(usize, usize, bool),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Apps,
    Background,
    Details,
}
pub fn display_rows(
    processes: &[Process],
    order: &[usize],
    matching: &HashSet<u32>,
    folded: &HashSet<(u32, u64)>,
    searching: bool,
    tree: bool,
    view: View,
) -> Vec<DisplayRow> {
    if view == View::Details {
        return order.iter().filter(|&&i| matching.contains(&processes[i].pid)).map(|&i| DisplayRow::Process(i, 0, false)).collect();
    }
    let by_pid: HashMap<_, _> = processes.iter().map(|p| (p.pid, p)).collect();
    // Cache ancestry so even long supervisor chains are visited once. A helper
    // belongs to its topmost application, rather than becoming another app.
    let mut owners: HashMap<u32, Option<u32>> = HashMap::new();
    for p in processes {
        let mut next = p.pid;
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        while let Some(p) = by_pid.get(&next) {
            if owners.contains_key(&next) || !visited.insert(next) {
                break;
            }
            path.push(*p);
            next = p.parent;
        }
        let mut owner = owners.get(&next).copied().flatten();
        for p in path.into_iter().rev() {
            owner = owner.or_else(|| p.app.then_some(p.pid));
            owners.insert(p.pid, owner);
        }
    }
    let in_apps: HashSet<_> = owners.iter().filter_map(|(&pid, owner)| owner.map(|_| pid)).collect();
    let mut result = Vec::new();
    let apps = view == View::Apps;
    let group: Vec<_> = order.iter().copied().filter(|&i| in_apps.contains(&processes[i].pid) == apps).collect();
    let rows = if tree {
        tree_rows(processes, &group, matching, folded, searching)
    } else {
        group.into_iter().filter(|&i| matching.contains(&processes[i].pid)).map(|i| (i, 0, false)).collect()
    };
    // Apps counts application roots, not browser/helper subprocesses.
    // Background processes counts individual processes, including folded ones.
    let count = if apps {
        matching.iter().filter_map(|pid| owners.get(pid).copied().flatten()).collect::<HashSet<_>>().len()
    } else {
        matching.iter().filter(|pid| !in_apps.contains(pid)).count()
    };
    if !searching || !rows.is_empty() {
        result.push(DisplayRow::Section(apps, count));
        result.extend(rows.into_iter().map(|(i, d, c)| DisplayRow::Process(i, d, c)));
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn process(pid: u32, parent: u32) -> Process {
        Process { pid, parent, start: pid as u64, ..Default::default() }
    }
    #[test]
    fn column_preferences_keep_order_and_required_name() {
        let details = Column::details();
        let value = details.iter().map(|c| c.key()).collect::<Vec<_>>().join(",");
        assert_eq!(Column::parse_list(&value), details);
        assert_eq!(Column::parse_list("PID,unknown,PID"), [Column::Name, Column::Pid]);
    }
    #[test]
    fn apps_detach_from_supervisors_and_keep_helpers_and_search() {
        let mut app = process(20, 1);
        app.app = true;
        let ps = vec![process(1, 0), app, process(21, 20), process(30, 1)];
        let matching = [1, 20, 21, 30].into_iter().collect();
        let rows = display_rows(&ps, &[0, 1, 2, 3], &matching, &HashSet::new(), false, true, View::Apps);
        assert_eq!(rows, [DisplayRow::Section(true, 1), DisplayRow::Process(1, 0, true), DisplayRow::Process(2, 1, false)]);
        let rows = display_rows(&ps, &[0, 1, 2, 3], &matching, &HashSet::new(), false, true, View::Background);
        assert_eq!(rows, [DisplayRow::Section(false, 2), DisplayRow::Process(0, 0, true), DisplayRow::Process(3, 1, false)]);
        let rows = display_rows(&ps, &[0, 1, 2, 3], &[21].into_iter().collect(), &[(20, 20)].into_iter().collect(), true, true, View::Apps);
        assert_eq!(rows, [DisplayRow::Section(true, 1), DisplayRow::Process(1, 0, true), DisplayRow::Process(2, 1, false)]);
        assert!(display_rows(&ps, &[0, 1, 2, 3], &[21].into_iter().collect(), &HashSet::new(), true, true, View::Background).is_empty());
        assert_eq!(
            display_rows(&ps, &[0, 1, 2, 3], &matching, &HashSet::new(), false, false, View::Apps),
            [DisplayRow::Section(true, 1), DisplayRow::Process(1, 0, false), DisplayRow::Process(2, 0, false)]
        );
        assert_eq!(
            display_rows(&ps, &[0, 1, 2, 3], &matching, &HashSet::new(), false, false, View::Background),
            [DisplayRow::Section(false, 2), DisplayRow::Process(0, 0, false), DisplayRow::Process(3, 0, false)]
        );
        assert_eq!(display_rows(&ps, &[0, 1, 2, 3], &matching, &HashSet::new(), false, false, View::Details).len(), 4);
    }
    #[test]
    fn hierarchy_search_and_collapse() {
        let ps = vec![process(1, 0), process(2, 1), process(3, 2), process(4, 99)];
        let order = [0, 3, 2, 1];
        let matches = [1, 2, 3, 4].into_iter().collect();
        let folded = [(2, 2)].into_iter().collect();
        let rows = tree_rows(&ps, &order, &matches, &folded, false);
        assert_eq!(rows.iter().map(|r| (ps[r.0].pid, r.1)).collect::<Vec<_>>(), [(1, 0), (2, 1), (4, 0)]);
        let rows = tree_rows(&ps, &order, &[3].into_iter().collect(), &folded, true);
        assert_eq!(rows.iter().map(|r| (ps[r.0].pid, r.1)).collect::<Vec<_>>(), [(1, 0), (2, 1), (3, 2)]);
    }
    #[test]
    fn cycles_do_not_lose_or_duplicate_processes() {
        let ps = vec![process(1, 2), process(2, 1), process(3, 3)];
        let rows = tree_rows(&ps, &[0, 1, 2], &[1, 2, 3].into_iter().collect(), &HashSet::new(), false);
        assert_eq!(rows.len(), 3);
    }
}
