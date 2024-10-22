# SDK Examples

Explore Hoku's functionalities through these practical examples.

All the examples target an `hoku` testnet subnet anchored to the Filecoin Calibration network.
You can run them with `cargo run --example [example name] - [ARG]`.

## Accounts

### Create a new account

[`account_create.rs`](account_create.rs) creates a random local private key for use in the other examples.

```shell
cargo run --example account_create
```

Example output:

```text
Private key: b630e3ffca6a5a35378520f5d803b1ad8622663420e91198ad16da546e041ed6
Address: 0x9c094a4a1376d24cb83667567dec2d6b2ba4944e
FVM address: t410ftqeuusqto3jezobwm5lh33bnnmv2jfcoijdi44q
```

### Deposit funds

To create transactions in the `hoku` testnet, you need to first deposit some Calibration tFIL in the `hoku` subnet.

1. Go to the [Calibration faucet](https://faucet.calibnet.chainsafe-fil.io/) and click "Send Funds".
2. Enter an Ethereum address.
   This can be any valid Ethereum address,
   like the one given by running the [`account_create.rs`](account_create.rs) example.
3. Look up the address you used on the [Calibration explorer](https://calibration.filfox.info/en).
   After about a minute, you should have 100 tFIL.
4. Now you're ready to make a deposit to the `hoku` testnet subnet.

Run the [`account_deposit.rs`](account_deposit.rs) example using the private key for the address you used above.

```shell
cargo run --example account_deposit -- [YOUR_HEX_ENCODED_PRIVATE_KEY]
```

Example output:

```text
Deposited 1 tFIL to 0x9c094a4a1376d24cb83667567dec2d6b2ba4944e
Transaction hash: 0x03d40fc3e8d629b2b52805e4fd1fb93f2d31bb06feaa3da55408091cbde6a654
```

### Check account balance

[`account_balance.rs`](account_balance.rs) shows the balance of an account in the `hoku` testnet subnet.
If you ran the [`account_deposit.rs`](account_deposit.rs), this should show a non-zero balance after a few minutes.

```shell
cargo run --example account_balance -- [YOUR_HEX_ENCODED_PRIVATE_KEY]
```

Example output:

```text
Balance of 0x9c094a4a1376d24cb83667567dec2d6b2ba4944e: 1.0
```

Read the docs (run `make doc` from the repo root) for more account methods.

### Buckets

[`bucket_add.rs`](bucket_add.rs) creates a new bucket, adds an object, and then queries for it by key.
To run this example, you must deposit some funds into the `hoku` testnet subnet.

```shell
cargo run --example bucket_add -- [YOUR_HEX_ENCODED_PRIVATE_KEY]
```

Example output:

```text
Created new bucket t2stfvbana4ljpvxdxit7ls23tt42owa2uy4isveq
Transaction hash: 0x005B93799842089F1AF25304D38FAD9995D5AC1C67D940A38E02383E984CADFB
Added 1MiB file to bucket t2stfvbana4ljpvxdxit7ls23tt42owa2uy4isveq with key foo/my_file
Transaction hash: 0xC49C5E0FBC62774C0A3C4AD24D4151D996F8BC9168C369A1A67D9049DA0A0278
Query result for key foo/my_file: bafybeidm37d6cxxoyu5fpadpuycta2wenno6ogmzi7uh3gsfd4e4c6tyda
```

Read the docs (run `make doc` from the repo root) for more bucket methods.

### Timehubs

[`timehub_push.rs`](timehub_push.rs) creates a new timehub for state updates, pushes a new value,
gets it back, and then qeuries for the timehub's count and state root.
To run this example, you must deposit some funds into the `hoku` testnet subnet.

```shell
cargo run --example timehub_push -- [YOUR_HEX_ENCODED_PRIVATE_KEY]
```

Example output:

```text
Created new timehub t2hmsh5t5rhrlhsl3igo35rh35lt4woboxzghpujq
Transaction hash: 0x54ECC3EA468D7FA3FAD165B8EAE6DEB0FECD88C53738E5E1C32BE3925E7AB886
Pushed to timehub t2hmsh5t5rhrlhsl3igo35rh35lt4woboxzghpujq with index 0
Transaction hash: 0x1D5DF1C9BB5CA5C5F14F8AC1D17188BB0EFE3D735A8A03A16EBB331D1FDEF0BF
Value at index 0: 'my_value'
Count: 1
State root: bafy2bzacecqsdwyjka2novzw77zex3mumho7r7q6ddcx7vgzy75fe5zqsbkxo
```

Read the docs (run `make doc` from the repo root) for more timehub methods.
