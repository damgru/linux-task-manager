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
        "Start" => "Uruchom",
        "Stop" => "Zatrzymaj",
        "Restart" => "Uruchom ponownie",
        "Refresh" => "Odśwież",
        "Service" => "Usługa",
        "Stopped" => "Zatrzymany",

        "Projects" => "Projekty",
        "Containers" => "Kontenery",
        "Images" => "Obrazy",
        "Project" => "Projekt",
        "Container details" => "Szczegóły kontenera",
        "Image details" => "Szczegóły obrazu",
        "Docker Compose project" => "Projekt Docker Compose",
        "Standalone containers" => "Kontenery samodzielne",
        "Running containers" => "Uruchomione kontenery",
        "Stopped containers" => "Zatrzymane kontenery",
        "Total Docker CPU" => "CPU Dockera łącznie",
        "Total Docker memory" => "RAM Dockera łącznie",
        "Docker totals include all containers, regardless of search. Sizes exclude volumes; root filesystems and images share layers." => "Sumy obejmują wszystkie kontenery, niezależnie od wyszukiwania. Rozmiary nie obejmują wolumenów; systemy plików i obrazy współdzielą warstwy.",
        "Images size" => "Rozmiar obrazów",
        "Unused" => "Nieużywany",
        "Usage" => "Użycie",
        "Dependent images" => "Obrazy zależne",
        "Layers" => "Warstwy",
        "Platform" => "Platforma",
        "Size" => "Rozmiar",
        "Writable size" => "Warstwa zapisywalna",
        "Image" => "Obraz",
        "Image ID" => "ID obrazu",
        "Container ID" => "ID kontenera",
        "Tags" => "Tagi",
        "Digests" => "Sumy kontrolne",
        "Parent image" => "Obraz nadrzędny",
        "Created" => "Utworzono",
        "Started" => "Uruchomiono",
        "Finished" => "Zakończono",
        "Health" => "Kondycja",
        "Exit code" => "Kod wyjścia",
        "Restart policy" => "Polityka restartu",
        "Memory limit" => "Limit pamięci",
        "Root filesystem size" => "Rozmiar systemu plików",
        "Networks" => "Sieci",
        "Ports" => "Porty",
        "Mounts" => "Montowania",
        "Compose directory" => "Katalog Compose",
        "Recent logs" => "Ostatnie logi",
        "Block I/O" => "Dyskowe I/O",
        "Block I/O (R / W)" => "Dysk (odczyt / zapis)",
        "Network I/O (Rx / Tx)" => "Sieć (odbiór / wysyłka)",
        "Disk I/O rate" => "Transfer dyskowy",
        "Force stop" => "Wymuś zatrzymanie",
        "Docker action" => "Operacja Docker",
        "Delete" => "Usuń",
        "images" => "obrazów",
        "Permanently delete these containers and their writable data? Volumes and images are kept." => "Trwale usunąć te kontenery i zapisane w nich dane? Wolumeny i obrazy zostaną zachowane.",
        "Force deletion sends SIGKILL immediately. Unsaved data will be lost." => "Wymuszone usunięcie natychmiast wysyła SIGKILL. Niezapisane dane zostaną utracone.",
        "Force delete running containers" => "Wymuś usunięcie działających kontenerów",
        "Permanently delete this image? Docker may refuse images with container references, dependent images or multiple tags." => "Trwale usunąć ten obraz? Docker może odmówić, jeśli obraz jest używany przez kontenery, ma obrazy zależne lub wiele tagów.",
        "containers" => "kontenerów",
        "running" => "uruchomionych",
        "exited" => "zatrzymany",
        "paused" => "wstrzymany",
        "restarting" => "restartowanie",
        "created" => "utworzony",
        "dead" => "martwy",
        "healthy" => "prawidłowa",
        "unhealthy" => "nieprawidłowa",
        "starting" => "sprawdzanie",
        "Search projects, containers or images" => "Szukaj projektów, kontenerów lub obrazów",
        "Detecting Docker…" => "Wykrywanie Dockera…",
        "Docker is not installed." => "Docker nie jest zainstalowany.",
        "Reading Docker inventory…" => "Odczytywanie danych Dockera…",
        "No matching Docker items." => "Brak pasujących elementów Dockera.",
        "No logs available." => "Brak dostępnych logów.",
        "Reading logs…" => "Odczytywanie logów…",
        "Logs are stale until refresh succeeds." => "Logi są nieaktualne do czasu udanego odświeżenia.",
        "Last 40 lines per container · up to 8 containers · refreshes with inventory" => "Ostatnie 40 linii na kontener · maks. 8 kontenerów · odświeżane wraz z danymi",
        "CPU: 100% = one logical CPU. I/O counters are cumulative. — = not reported." => "CPU: 100% = jeden procesor logiczny. Liczniki I/O są sumaryczne. — = brak danych.",
        "Image sizes include shared layers; do not add them to estimate disk usage." => "Rozmiary obrazów obejmują współdzielone warstwy; ich suma nie odpowiada zajętości dysku.",
        "Dependent images share the complete base layer prefix; build ancestry is not always retained." => "Obrazy zależne zawierają wszystkie początkowe warstwy obrazu bazowego; historia budowania nie zawsze jest zachowana.",
        "Force stop sends SIGKILL immediately. Unsaved data may be lost." => "Wymuszone zatrzymanie natychmiast wysyła SIGKILL. Niezapisane dane mogą zostać utracone.",
        "Wait up to 10 seconds, then force stop any remaining containers." => "Poczekaj do 10 sekund, a następnie wymuś zatrzymanie pozostałych kontenerów.",
        "Docker is unavailable. Check that the daemon is running and your user has socket access." => "Docker jest niedostępny. Sprawdź, czy demon działa i czy użytkownik ma dostęp do gniazda.",
        "Previously collected data is stale. Actions are disabled until refresh succeeds." => "Wcześniejsze dane są nieaktualne. Operacje będą dostępne po udanym odświeżeniu.",
        "Container removed" => "Kontener usunięty",
        "Image removed" => "Obraz usunięty",
        "Invalid Docker identifier" => "Nieprawidłowy identyfikator Dockera",
        "Docker inventory exceeds the safe limit of 256 items." => "Przekroczono bezpieczny limit 256 elementów Dockera.",

        "The command output was too large to verify. Use your software manager." => "Wynik polecenia jest zbyt duży do weryfikacji. Użyj menedżera oprogramowania.",
        "Parent process" => "Proces nadrzędny",
        "Open parent process" => "Przejdź do procesu nadrzędnego",
        "Check for updates" => "Sprawdź aktualizacje",
        "Checking for updates…" => "Sprawdzanie aktualizacji…",
        "Updating…" => "Aktualizowanie…",
        "Update" => "Aktualizuj",
        "Update all system packages" => "Aktualizuj wszystkie pakiety systemowe",
        "Available version" => "Dostępna wersja",
        "Update available" => "Dostępna aktualizacja",
        "No updates found in the available package sources." => "Nie znaleziono aktualizacji w dostępnych źródłach pakietów.",
        "Arch Linux requires upgrading all system packages together." => "Arch Linux wymaga jednoczesnej aktualizacji wszystkich pakietów systemowych.",
        "Uses cached package sources. Refresh them in your software manager for the latest results." => "Używane są zapisane źródła pakietów. Odśwież je w menedżerze oprogramowania, aby uzyskać najnowsze wyniki.",
        "Update checks are unavailable for this installation. Use its software manager." => "Ta instalacja nie obsługuje sprawdzania aktualizacji. Użyj jej menedżera oprogramowania.",
        "No package candidate is available. Refresh sources in your software manager." => "Brak dostępnej wersji pakietu. Odśwież źródła w menedżerze oprogramowania.",
        "Update completed. Restart the application to use the installed version." => "Aktualizacja zakończona. Uruchom aplikację ponownie, aby korzystać z zainstalowanej wersji.",
        "Measure network usage" => "Zmierz użycie sieci",
        "Click to authenticate and measure this process with NetHogs. * = last authorized sample, shown for 10 seconds." => "Kliknij, aby uwierzytelnić się i zmierzyć ruch procesu przez NetHogs. * = ostatnia autoryzowana próbka, widoczna przez 10 sekund.",
        "Network sample captured. The reading is shown for 10 seconds; click again to measure." => "Pobrano próbkę ruchu sieciowego. Odczyt jest widoczny przez 10 sekund; kliknij ponownie, aby wykonać pomiar.",
        "Per-process network measurement requires NetHogs. Install nethogs with your software manager and try again." => "Pomiar sieci dla procesu wymaga NetHogs. Zainstaluj nethogs w menedżerze oprogramowania i spróbuj ponownie.",
        "Network capture failed. Check NetHogs support and network permissions." => "Nie udało się zmierzyć ruchu. Sprawdź obsługę NetHogs i uprawnienia sieciowe.",
        "No traffic could be attributed to this process during the sample. Try again while it is using the network." => "Podczas pomiaru nie przypisano ruchu do tego procesu. Spróbuj ponownie, gdy korzysta z sieci.",
        "Unable to compare package versions." => "Nie udało się porównać wersji pakietów.",
        "The command timed out." => "Przekroczono czas oczekiwania na polecenie.",
        "Invalid network measurement" => "Nieprawidłowy pomiar sieci",
        "Unsupported package update" => "Nieobsługiwana aktualizacja pakietu",
        "Demo · fictional data" => "Demo · fikcyjne dane",
        "Demo mode: system actions are disabled." => "Tryb demo: operacje systemowe są wyłączone.",
        "<  Back" => "<  Wstecz",
        "Application details" => "Szczegóły aplikacji",
        "Properties" => "Właściwości",
        "Exited" => "Zakończony",
        "This process has exited. Its last details are kept until you go back." => "Proces został zakończony. Ostatnie szczegóły pozostaną widoczne do powrotu do listy.",
        "Resource usage · includes all subprocesses" => "Zużycie zasobów · łącznie ze wszystkimi podprocesami",
        "Running time" => "Czas działania",
        "Last 60 samples · history starts when this page opens" => "Ostatnie 60 próbek · historia od otwarcia tej strony",
        "Process information" => "Informacje o procesie",
        "Installation" => "Instalacja",
        "Owner" => "Właściciel",
        "Installed via" => "Sposób instalacji",
        "Package" => "Pakiet",
        "Installed version" => "Zainstalowana wersja",
        "Reading package information…" => "Odczytywanie informacji o pakiecie…",
        "No supported package record found; this may be a manually installed application." => "Nie znaleziono wpisu pakietu; aplikacja mogła zostać zainstalowana ręcznie.",
        "Executable unavailable; package information could not be verified." => "Plik wykonywalny jest niedostępny; nie można sprawdzić informacji o pakiecie.",
        "Executable and command line" => "Plik wykonywalny i wiersz polecenia",
        "Subprocesses" => "Podprocesy",
        "Double-click to open details" => "Kliknij dwukrotnie, aby otworzyć szczegóły",
        "No subprocesses" => "Brak podprocesów",
        "Reduce priority" => "Obniż priorytet",
        "Suspend pauses execution; Resume continues it." => "Wstrzymaj zatrzymuje wykonywanie; Wznów je kontynuuje.",
        "Close" => "Zamknij",
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
        "Total memory" => "Pamięć Razem",
        "Total CPU" => "CPU Razem",
        "CPU of this process and all descendants, including hidden ones, as a percentage of total system capacity. Click to sort; right-click for columns." => {
            "CPU procesu i wszystkich jego podprocesów, także ukrytych, jako procent całkowitej mocy systemu. Kliknij, aby sortować; PPM: kolumny."
        }
        "RSS of this process and all descendants, including hidden ones. Shared pages can be counted more than once. Click to sort; right-click for columns." => {
            "Suma RSS procesu i wszystkich jego podprocesów, także ukrytych. Strony współdzielone mogą być liczone wielokrotnie. Kliknij, aby sortować; PPM: kolumny."
        }
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
        "Drag column headings to reorder; drag their edges to resize." => "Przeciągnij nagłówki, aby zmienić kolejność; krawędzie, aby zmienić szerokość.",
        "Drag edges to resize · Drag to move · Click to sort · Right-click for columns" => "Przeciągnij krawędź: szerokość · Nagłówek: kolejność · Kliknij: sortowanie · PPM: kolumny",
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
        "This process priority change requires administrator access and cannot be safely authorized." => {
            "Zmiana priorytetu tego procesu wymaga uprawnień administratora, których nie można bezpiecznie autoryzować."
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
        "Footer" => "Stopka",
        "Choose which resource counters appear in the footer" => "Wybierz elementy widoczne w stopce",
        "selected" => "wybrano",
        "Process count" => "Liczba procesów",
        "Total number of system processes" => "Łączna liczba procesów w systemie",
        "Linux load averages over 1, 5 and 15 minutes; not a percentage. — means not reported." => {
            "Średnie obciążenie Linuksa z 1, 5 i 15 minut; nie jest procentem. — oznacza brak danych."
        }
        "Highest reported GPU utilization. — means not reported by the driver." => {
            "Najwyższe odczytane użycie GPU. — oznacza brak danych ze sterownika."
        }
        "Used / total physical memory" => "Używana / całkowita pamięć fizyczna",
        "Used / total swap; 0 KB / 0 KB means no swap is configured" => "Używana / całkowita pamięć swap; 0 KB / 0 KB oznacza brak swapu",
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
