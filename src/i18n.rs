use std::sync::atomic::{AtomicBool, Ordering};
static POLISH: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Auto,
    English,
    Polish,
}
impl Language {
    pub fn from(value: &str) -> Self {
        match value {
            "en" => Self::English,
            "pl" => Self::Polish,
            _ => Self::Auto,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::English => "en",
            Self::Polish => "pl",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Detect automatically",
            Self::English => "English",
            Self::Polish => "Polski",
        }
    }
}
pub fn locale_is_polish(value: &str) -> bool {
    let value = value.split(':').next().unwrap_or("").to_ascii_lowercase();
    value == "pl" || value.starts_with("pl_") || value.starts_with("pl-") || value.starts_with("pl.")
}
pub fn set_language(language: Language) {
    let polish = match language {
        Language::Polish => true,
        Language::English => false,
        Language::Auto => {
            let locale =
                ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty())).unwrap_or_default();
            // GNU LANGUAGE is a preference list, except in the C/POSIX locale.
            let preference =
                if locale == "C" || locale == "POSIX" { locale } else { std::env::var("LANGUAGE").ok().filter(|s| !s.is_empty()).unwrap_or(locale) };
            locale_is_polish(&preference)
        }
    };
    POLISH.store(polish, Ordering::Relaxed);
}
pub fn tr(s: &str) -> &str {
    if !POLISH.load(Ordering::Relaxed) {
        return s;
    }
    match s {
        "Demo · fictional data" => "Demo · fikcyjne dane",
        "Demo mode: system actions are disabled." => "Tryb demo: operacje systemowe są wyłączone.",
        "Task Manager" => "Menedżer zadań",
        "Processes" => "Procesy",
        "Performance" => "Wydajność",
        "App history" => "Historia aplikacji",
        "Startup apps" => "Autostart",
        "Users" => "Użytkownicy",
        "Details" => "Szczegóły",
        "Services" => "Usługi",
        "Settings" => "Ustawienia",
        "Name" => "Nazwa",
        "Type" => "Typ",
        "Status" => "Stan",
        "Memory" => "Pamięć",
        "Disk" => "Dysk",
        "Network" => "Sieć",
        "User name" => "Użytkownik",
        "User" => "Użytkownik",
        "System" => "System",
        "Threads" => "Wątki",
        "Priority" => "Priorytet",
        "Parent PID" => "PID rodzica",
        "Command line" => "Wiersz polecenia",
        "Disk read" => "Odczyt dysku",
        "Disk written" => "Zapis dysku",
        "GPU engine" => "Silnik GPU",
        "Power usage" => "Pobór energii",
        "Apps" => "Aplikacje",
        "Background processes" => "Procesy w tle",
        "Virtual network" => "Sieć wirtualna",
        "Hide virtual network adapters" => "Ukryj wirtualne karty sieciowe",
        "Performance shows only physical Wi-Fi and Ethernet adapters" => "Wydajność: tylko fizyczne karty Wi-Fi i Ethernet",
        "Drag column headings to reorder." => "Przeciągnij nagłówki, aby zmienić kolejność.",
        "Drag to move · Click to sort · Right-click for columns" => "Przeciągnij, aby przenieść · Kliknij, aby sortować · PPM: kolumny",
        "Move left" => "Przenieś w lewo",
        "Move right" => "Przenieś w prawo",
        "App" => "Aplikacja",
        "System process" => "Proces systemowy",
        "Background process" => "Proces w tle",
        "Running" => "Uruchomiony",
        "Sleeping" => "Uśpiony",
        "Suspended" => "Wstrzymany",
        "Idle" => "Bezczynny",
        "Unknown" => "Nieznany",
        "Efficiency" => "Oszczędny",
        "Run new task" => "Uruchom nowe zadanie",
        "⊞  Run new task" => "⊞  Nowe zadanie",
        "End task" => "Zakończ zadanie",
        "End task…" => "Zakończ zadanie…",
        "Efficiency mode" => "Tryb oszczędny",
        "Tree view" => "Drzewo",
        "List view" => "Lista",
        "Process tree" => "Drzewo procesów",
        "Expand all" => "Rozwiń wszystko",
        "Collapse all" => "Zwiń wszystko",
        "Columns" => "Kolumny",
        "Select columns" => "Wybierz kolumny",
        "Reset columns" => "Przywróć kolumny",
        "Unavailable counters are shown as —." => "Niedostępne odczyty oznaczono —.",
        "This per-process counter is not available from the current Linux driver or permissions." => {
            "Ten odczyt dla procesu jest niedostępny przy obecnym sterowniku lub uprawnieniach."
        }
        "No matching processes" => "Brak pasujących procesów",
        "Try another name, user or process ID." => "Wpisz inną nazwę, użytkownika lub PID.",
        "Type a name, user, or PID to search" => "Wyszukaj nazwę, użytkownika lub PID",
        "Resume updates" => "Wznów odświeżanie",
        "Pause updates" => "Wstrzymaj odświeżanie",
        "Refresh only reads Linux system data." => "Odświeżanie odczytuje dane systemu Linux.",
        "Reduce scheduling priority (nice +10)" => "Obniż priorytet procesu (nice +10)",
        "Resume" => "Wznów",
        "Suspend" => "Wstrzymaj",
        "Copy command line" => "Kopiuj wiersz polecenia",
        "Go to details" => "Przejdź do szczegółów",
        "Action completed." => "Operacja zakończona.",
        "Task started." => "Zadanie uruchomione.",
        "This process is protected." => "Ten proces jest chroniony.",
        "The process exited or changed. Select it again." => "Proces zakończył się lub zmienił. Wybierz go ponownie.",
        "Administrator permission required" => "Wymagane uprawnienia administratora",
        "This operation needs administrator permission. Authenticate using the system dialog to continue." => {
            "Ta operacja wymaga uprawnień administratora. Aby kontynuować, uwierzytelnij się w oknie systemowym."
        }
        "Authenticate…" => "Uwierzytelnij…",
        "Cancel" => "Anuluj",
        "Run" => "Uruchom",
        "Command" => "Polecenie",
        "e.g. xterm" => "np. xterm",
        "System authentication is unavailable. Install polkit and enable a desktop authentication agent." => {
            "Uwierzytelnianie systemowe jest niedostępne. Zainstaluj Polkit i włącz agenta uwierzytelniania pulpitu."
        }
        "Authentication cancelled. No changes were made." => "Anulowano uwierzytelnianie. Nie wprowadzono zmian.",
        "Authentication was denied or no desktop authentication agent is running." => {
            "Odmówiono uwierzytelnienia lub agent uwierzytelniania pulpitu nie działa."
        }
        "Enter a command to start an application or open a terminal." => "Wpisz polecenie, aby uruchomić aplikację lub terminal.",
        "Runs with your current Linux user's permissions." => "Uruchamianie z uprawnieniami bieżącego użytkownika Linuksa.",
        "End this process?" => "Zakończyć ten proces?",
        "Unsaved work in this process may be lost." => "Niezapisane dane tego procesu mogą zostać utracone.",
        "Force quit if unresponsive (SIGKILL)" => "Wymuś zakończenie, jeśli nie odpowiada (SIGKILL)",
        "Force quit" => "Wymuś zakończenie",
        "The process will stop immediately." => "Proces zostanie natychmiast zatrzymany.",
        "First ask the process to exit normally (SIGTERM)." => "Najpierw poproś proces o zwykłe zakończenie (SIGTERM).",
        "Appearance" => "Wygląd",
        "App theme" => "Motyw aplikacji",
        "Select which app theme to display" => "Wybierz motyw aplikacji",
        "Dark" => "Ciemny",
        "Light" => "Jasny",
        "General" => "Ogólne",
        "Language" => "Język",
        "Choose the application language" => "Wybierz język aplikacji",
        "Detect automatically" => "Wykrywaj automatycznie",
        "Default start page" => "Strona początkowa",
        "Choose the page shown when opening Task Manager" => "Wybierz widok po uruchomieniu Menedżera zadań",
        "Real time update speed" => "Szybkość odświeżania",
        "Choose how often system resource usage is refreshed" => "Wybierz częstotliwość odświeżania zużycia zasobów",
        "Paused" => "Wstrzymane",
        "High" => "Wysoka",
        "Low" => "Niska",
        "Normal" => "Normalna",
        "High · 0.5 s" => "Wysoka · 0,5 s",
        "Normal · 1 s" => "Normalna · 1 s",
        "Low · 2 s" => "Niska · 2 s",
        "Low memory mode" => "Tryb oszczędzania pamięci",
        "Refresh every 5 seconds, disable graphs and session history" => "Odświeżaj co 5 sekund, wyłącz wykresy i historię sesji",
        "Enabled" => "Włączone",
        "Disabled" => "Wyłączone",
        "Window management" => "Zarządzanie oknem",
        "Set the default window behavior" => "Ustaw domyślne zachowanie okna",
        "Always on top" => "Zawsze na wierzchu",
        "Other options" => "Inne opcje",
        "Process actions and resource collection" => "Operacje na procesach i zbieranie danych",
        "Ask before ending a process" => "Pytaj przed zakończeniem procesu",
        "CPU is shown as a percentage of total system capacity." => "CPU jest pokazywane jako procent całkowitej mocy systemu.",
        "About" => "Informacje",
        "Task Manager for Linux" => "Menedżer zadań dla Linuksa",
        "Rust + egui · native rendering · no browser runtime" => "Rust + egui · natywne renderowanie",
        "Updates paused" => "Odświeżanie wstrzymane",
        "Low memory mode · 5 s" => "Tryb oszczędny · 5 s",
        "Clear history" => "Wyczyść historię",
        "Enable / disable" => "Włącz / wyłącz",
        "↻  Restart" => "↻  Uruchom ponownie",
        "□  Stop" => "□  Zatrzymaj",
        "▷  Start" => "▷  Uruchom",
        "start" => "uruchom",
        "stop" => "zatrzymaj",
        "restart" => "uruchom ponownie",
        "Description" => "Opis",
        "Scope" => "Zakres",
        "Source" => "Źródło",
        "CPU time" => "Czas CPU",
        "Memory (sum of RSS)" => "Pamięć (suma RSS)",
        "Not reported" => "Brak danych",
        "Utilization" => "Wykorzystanie",
        "Speed" => "Szybkość",
        "Up time" => "Czas pracy",
        "Memory usage" => "Użycie pamięci",
        "Memory composition" => "Skład pamięci",
        "In use" => "W użyciu",
        "Available" => "Dostępna",
        "Cached" => "Pamięć podręczna",
        "Swap used" => "Swap w użyciu",
        "Swap total" => "Całkowity swap",
        "Active time" => "Czas aktywności",
        "Read speed" => "Szybkość odczytu",
        "Write speed" => "Szybkość zapisu",
        "Throughput" => "Przepływność",
        "Send" => "Wysyłanie",
        "Receive" => "Odbieranie",
        "60 seconds" => "60 sekund",
        "Dedicated GPU memory" => "Dedykowana pamięć GPU",
        "Temperature" => "Temperatura",
        "GPU utilization" => "Wykorzystanie GPU",
        "Video Encode" => "Kodowanie wideo",
        "Video Decode" => "Dekodowanie wideo",
        "Power" => "Moc",
        "Driver" => "Sterownik",
        "No compatible device detected" => "Nie wykryto obsługiwanego urządzenia",
        "GPU utilization is unavailable from the current driver." => "Bieżący sterownik nie udostępnia wykorzystania GPU.",
        "NVIDIA telemetry is unavailable. Check that the NVIDIA driver and nvidia-smi work in this session." => {
            "Odczyty NVIDIA są niedostępne. Sprawdź działanie sterownika NVIDIA i nvidia-smi w tej sesji."
        }
        "No GPU telemetry is available on this Linux system.\nThe GPU page uses real driver data when the device exposes it." => {
            "Brak odczytów GPU w tym systemie Linux.\nDane pojawią się, gdy sterownik je udostępni."
        }
        "Graphs are disabled in low memory mode." => "Wykresy są wyłączone w trybie oszczędzania pamięci.",
        "Live" => "Na żywo",
        "now" => "teraz",
        "Group" => "Grupa",
        "Logical processors" => "Procesory logiczne",
        "% Utilization" => "% wykorzystania",
        "Architecture" => "Architektura",
        "Collector time" => "Czas odczytu",
        "Adapter name" => "Nazwa interfejsu",
        "Connection" => "Połączenie",
        "MAC address" => "Adres MAC",
        "Received" => "Odebrano",
        "Sent" => "Wysłano",
        "Capacity" => "Pojemność",
        "Device" => "Urządzenie",
        "SSD / virtual" => "SSD / wirtualny",
        "Could not save preferences" => "Nie można zapisać ustawień",
        "Reading services…" => "Odczytywanie usług…",
        "No systemd services were found." => "Nie znaleziono usług systemd.",
        "No XDG autostart applications were found.\nDesktop startup entries will appear here when installed." => {
            "Nie znaleziono aplikacji autostartu XDG.\nWpisy pojawią się po zainstalowaniu aplikacji."
        }
        "Graphs are paused in low memory mode.\nCurrent readings remain available below." => {
            "Wykresy są wstrzymane w trybie oszczędzania pamięci.\nBieżące odczyty są dostępne poniżej."
        }
        "samples · history since launch" => "próbek · historia od uruchomienia",
        "Resource usage since this app opened" => "Zużycie zasobów od uruchomienia",
        "session only" => "tylko bieżąca sesja",
        "Showing the first 64 logical processors." => "Pokazano pierwsze 64 procesory logiczne.",
        "No GPU exposed by driver" => "Sterownik nie udostępnia GPU",
        "Process data: /proc · GPU: nvidia-smi / sysfs · Services: systemd" => "Dane procesów: /proc · GPU: nvidia-smi / sysfs · Usługi: systemd",
        "active" => "aktywna",
        "inactive" => "nieaktywna",
        "failed" => "błąd",
        "activating" => "uruchamianie",
        "deactivating" => "zatrzymywanie",
        "up" => "połączono",
        "down" => "rozłączono",
        "User service authorization must be granted in the current user's systemd session." => {
            "Dostęp do usługi użytkownika musi zostać przyznany w jego bieżącej sesji systemd."
        }
        _ => s,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_supported_locales() {
        for locale in ["pl_PL.UTF-8", "pl-PL", "pl", "pl:en_US"] {
            assert!(locale_is_polish(locale));
        }
        for locale in ["en_US.UTF-8", "C", "de_DE", "plausible"] {
            assert!(!locale_is_polish(locale));
        }
    }
}
