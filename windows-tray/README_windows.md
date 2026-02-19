# Compass Lunch (Windows)

## Run

```powershell
cargo run --manifest-path windows-tray/Cargo.toml
```

## Flags

- `--print-today` fetch + parse and print today's menu to stdout
- `--no-tray` show the popup as a normal window without a tray icon

## Settings and Cache

- Settings: `%LOCALAPPDATA%\compass-lunch\settings.json`
- Cache: `%LOCALAPPDATA%\compass-lunch\cache\<costNumber>|<language>.json`

## Notes

- Default restaurant: `0437` (Snellmania)
- Default language: `fi`
- Default refresh: `1440` minutes
