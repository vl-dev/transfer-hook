# transfer-hook
The SPL Transfer Hook interface and its clients. This is a testing implementation for looging a number of tokens transfered by a user.

The implementation is by no means perfect as there is no reasonable way for the user ATA to be created as a part of the Execute instruction so it is done separately.

## Testing the whole flow

1. Generate a new mint keypair:
    ```
    solana-keygen new --outfile ./mint.json
    ```

2. Replace the mint address in `./program/src/lib.rs` with the public key of the created mint

3. Run the following series of commands
    ``` 
    make build-sbf-program && ./scripts/restart-test-validator.sh && ./token-flow.sh
    ```