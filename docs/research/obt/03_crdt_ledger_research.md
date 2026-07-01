# Research: CRDT-Based & DAG-Based Ledger Designs

> Researched: 30/06/2026 | Sources: Nano, IOTA, Holochain, GOC-Ledger paper

## Comparison Table

| Aspect | Nano (XNO) | IOTA (Tangle) | Holochain |
|--------|-----------|--------------|-----------|
| **Structure** | Block-lattice (each account = own chain) | DAG (no blocks, no chains) | Agent-centric (each node = own chain + DHT) |
| **Consensus** | Open Representative Voting (ORV), >66% quorum | BFT committee + delegated stake | No global consensus — DNA validation rules |
| **Fees** | Zero | Zero (Mana-based rate limiting) | Zero |
| **Double-spend** | Fork detection → ORV voting → cement | Tip selection + BFT finality | Source chain linearity, peer warrants |
| **Transaction** | 2-phase: Send block + Receive block | Single tx references 2 tips | Double-entry on both chains |
| **Finality** | Deterministic (~0.2-1s) | Deterministic (BFT in 2.0) | Eventual (DHT gossip) |
| **Scalability** | Parallel per account | Theoretically infinite | Unlimited (each agent own chain) |
| **Balance** | State block records resulting balance | UTXO or account-based | Mutual credit (sum=0) |

## Critical Finding: G-Counter Cannot Work for OBT Balance

**G-Counter**: only increment → cannot spend
**PNCounter**: allows concurrent decrements → overdraft possible (balance can go negative)

```
Example: PNCounter failure
  Node A sees balance=100, spends 80
  Node B sees balance=100, spends 80
  After CRDT merge: balance = 100 - 80 - 80 = -60 ← INVALID
```

## Recommended: Account-Chain Model (Nano-inspired)

### Data Structure

```rust
pub struct TransferBlock {
    pub previous: [u8; 32],      // hash of prev block ([0;32] for genesis)
    pub account: [u8; 32],       // Ed25519 public key
    pub sequence: u64,           // monotonically increasing
    pub balance: u64,            // balance AFTER this operation
    pub operation: TransferOp,   // Open | Mint | Send | Receive
    pub clock: VectorClock,      // causal ordering
    pub timestamp: u64,
    pub signature: [u8; 64],     // Ed25519
    pub block_hash: [u8; 32],    // BLAKE3 of this block
}

pub enum TransferOp {
    Open,
    Mint { source: MintSource, amount: u64 },
    Send { receiver: [u8; 32], amount: u64 },
    Receive { send_block_hash: [u8; 32], amount: u64 },
}
```

### Why Account-Chain Wins for OBP

1. Leverages ALL existing primitives (Ed25519, BLAKE3, DHT, VectorClock)
2. Single-writer = no coordination for writes
3. DHT validation = no global consensus needed
4. G-Counters still useful for analytics (total_earned, total_spent)
5. Near-infinite supply = minting is just another TransferOp
6. Fee-less (no mining)
7. Compatible with existing reward system

### Transfer Flow (2-Phase)

```
SENDER (Alice):                    RECEIVER (Bob):
1. Create Send block               4. See pending Send (DHT)
   balance = old - amount          5. Create Receive block
2. Sign Ed25519                       balance = old + amount
3. Broadcast to DHT               6. Sign Ed25519
   → Neighbors validate           7. Broadcast to DHT
```

### Double-Spend Prevention

```
Alice creates TWO Send blocks with same sequence
→ DHT neighbors detect FORK
→ Accept FIRST seen, reject second
→ Tiebreak: lower block_hash wins
→ Issue "warrant" (cryptographic proof of cheating)
```

### Where CRDTs Still Apply

| CRDT | Used For |
|------|---------|
| GCounter | total_earned, total_spent (analytics) |
| GCounter | Global supply counter |
| LWWRegister | Account metadata |
| ORSet | Pending unreceived Send blocks |
| VectorClock | Causal ordering across accounts |
