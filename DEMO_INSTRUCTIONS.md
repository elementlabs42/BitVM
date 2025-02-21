TODO: Add example `bridge.toml` and say where to put it.
Explain how to run all the required clients from the same machine.

#### Initiate peg-in

`<TXID>:<VOUT>` = Bridge deposit UTXO that includes the expected peg-in amount. It must be spendable by the depositor private key. Suggested test amount: `2100000 sats`.

```
-n -u <TXID>:<VOUT> -d <EVM_ADDRESS>
```

#### Create peg-out graph

```
-t -u <TXID>:<VOUT> -i <PEG_IN_GRAPH_ID>
```

#### Push verifier_0 nonces for peg-in graph

```
-c -i <GRAPH_ID>
```

#### Push verifier_1 nonces for peg-in graph

```
-c -i <GRAPH_ID>
```

#### Push verifier_0 signatures for peg-in graph

```
-g -i <GRAPH_ID>
```

#### Push verifier_1 signatures for peg-in graph

```
-g -i <GRAPH_ID>
```

#### Broadcast peg-in confirm

```
-b pegin -g <PEG_IN_GRAPH_ID> confirm
```

#### Push verifier_0 nonces for peg-out graph

```
-c -i <GRAPH_ID>
```

#### Push verifier_1 nonces for peg-out graph

```
-c -i <GRAPH_ID>
```

#### Push verifier_0 signatures for peg-out graph

```
-g -i <GRAPH_ID>
```

#### Push verifier_1 signatures for peg-out graph

```
-g -i <GRAPH_ID>
```

#### Mock L2 chain service (using peg-in confirm tx)

```
-x -u <TXID>:<VOUT>
```

#### Broadcast peg-out

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> peg_out
```

#### Broadcast peg-out confirm

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> peg_out_confirm
```

#### Broadcast kick-off 1

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> kick_off_1
```

#### Broadcast kick-off 2

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> kick_off_2
```

#### Broadcast assert-initial

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> assert_initial
```

#### Broadcast assert-commit 1

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> assert_commit_1
```

#### Broadcast assert-commit 2

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> assert_commit_2
```

#### Broadcast assert-final

```
-b tx -g <GRAPH_ID> -u <TXID>:<VOUT> assert_final
```

#### Broadcast disprove

```
-b tx -g <GRAPH_ID> -a <BTC_ADDRESS> disprove
```
