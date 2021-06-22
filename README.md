# Substrate-Moloch-V2

Moloch DAO implementation in substrate, it's based on substrate v2.0.1.

## Introduction  

The application implmented [V2 protocol of Moloch](https://github.com/MolochVentures/moloch/blob/master/).In this version, there are 2 major enhancements.
1. Anyone can submit a proposal, but only sponsored proposal can be pushed into voting/processing queue. This means to mitigate the risk of token approval.
2. The contracts can support multiple tokens at the same time. So there are dedicated proposals to accept new tokens.

As multiple tokens are not implemented yet in substrate, we'll skip this part in our pallet. 

## Test
For unit test, just run
```
cargo test -p pallet-moloch-v2
````
For an integration/simulation test, please follow our detailed [test guide](./doc/test-guide.md)
