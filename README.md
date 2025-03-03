# Liquidity Provisioning for USDC/USDT Pool

This guide outlines the steps required to import your wallet and manage liquidity in the USDC/USDT pool. Follow each section carefully to ensure a smooth setup.

## Prerequisites

- **Phantom Wallet:** Installed and configured.
- **Solana CLI:** Installed and properly configured.
- **Cargo:** Installed for running Rust commands.
- **Wallet Balance:** Ensure you have at least **1 USDC** and **1 USDT** in your wallet.

## Reference Documentation

- **Save Finance (Mainnet Addresses):**  
  [Documentation](https://docs.save.finance/architecture/addresses/mainnet/main-pools)

- **Raydium (Mainnet Addresses):**  
  [Documentation](https://docs.raydium.io/raydium/protocol/developers/addresses#raydium-programs)

## Wallet Recovery and Import

1. **Retrieve Your Recovery Phrase:**

   - Open Phantom Wallet.
   - Navigate to **Settings → Security & Privacy → Show Recovery Phrase**.
   - Securely store your recovery phrase.

2. **Import Your Wallet:**

   Run the following command to import your wallet using the recovery phrase:
   ```bash
   solana-keygen recover 'prompt:?key=0/0' --outfile ~/.config/solana/id.json
   ```

3. **Verify Wallet Import:**

   Check that your wallet is correctly imported by running:
   ```bash
   solana balance
   ```

## Liquidity Operations

### Provide Liquidity

To add liquidity to the USDC/USDT pool, execute:
```bash
cargo run -- open-position 0.999 1.001 500000
```
- **Parameters:**
  - `0.999`: Lower bound price.
  - `1.001`: Upper bound price.
  - `500000`: Liquidity amount (in smallest unit).

### Remove Liquidity

To remove liquidity from the USDC/USDT pool, execute:
```bash
cargo run -- close-position 0.999 1.001
```
- **Parameters:**
  - `0.999`: Lower bound price.
  - `1.001`: Upper bound price.

## Final Notes

- **Funding:** Confirm that your wallet has at least **1 USDC** and **1 USDT** before executing liquidity operations.
- **Documentation:** Refer to the provided links for more details on the network addresses and protocol configurations.
