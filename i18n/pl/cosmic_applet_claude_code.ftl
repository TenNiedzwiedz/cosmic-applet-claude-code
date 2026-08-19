applet-name = Claude Code
no-sessions = Brak aktywnych sesji
sessions = { $count ->
    [one] { $count } sesja
    [few] { $count } sesje
   *[many] { $count } sesji
}
status-busy = pracuje
status-idle = bezczynna
five-hour = Limit 5h
seven-day = Limit tygodniowy
resets-in = reset za { $time }
resets-at = reset { $time }
context = { $percent }% kontekstu
no-limits = Brak danych o zużyciu
no-limits-hint = Uruchom sesję Claude Code lub wykonaj `just install-bridge`, aby włączyć mostek wiersza stanu.
captured-ago = dane sprzed { $time }
open-settings = Ustawienia panelu
unknown = nieznane
weekday-mon = pon
weekday-tue = wt
weekday-wed = śr
weekday-thu = czw
weekday-fri = pt
weekday-sat = sob
weekday-sun = niedz
