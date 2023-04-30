# UULM Mensa Stuff
## CLI
Run CLI using `cargo run --bin uulm_mensa_cli`. The `MENSA_ID` for west is `2`.

```
Usage: uulm_mensa_cli <MENSA_ID> <COMMAND>

Commands:
  menu   
  slots  
  order  
  help   Print this message or the help of the given subcommand(s)

Arguments:
  <MENSA_ID>  

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Telegram Bot
Run telegram bot using `cargo run --bin uulm_mensa_bot`.
This requires a telegram bot token, which should be provided in a file called `.env`,
like that:

```
TELOXIDE_TOKEN="1xxxxxxxxx:Axxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
PERSISTENCE_SQLITE=1
RUST_LOG="warning,uulm_mensa_bot=debug"
#PRODUCTION=1
```
