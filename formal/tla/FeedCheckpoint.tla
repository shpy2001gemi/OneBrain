-------------------------- MODULE FeedCheckpoint --------------------------
EXTENDS Naturals, FiniteSets

Primary == {0, 1, 2}
Forks == {3, 4}
NoCheckpoint == 3

VARIABLES known, knownForks, covered, proofValid, suppressed, suppressedForks
vars == <<known, knownForks, covered, proofValid, suppressed, suppressedForks>>

Covered == IF covered = NoCheckpoint THEN {} ELSE {e \in Primary : e <= covered}

Init == /\ known = {}
        /\ knownForks = {}
        /\ covered = NoCheckpoint
        /\ proofValid = FALSE
        /\ suppressed = {}
        /\ suppressedForks = {}

Receive(e) == /\ e \in Primary
              /\ known' = known \cup {e}
              /\ UNCHANGED <<knownForks, covered, proofValid, suppressed, suppressedForks>>

ReceiveFork(f) == /\ f \in Forks
                  /\ knownForks' = knownForks \cup {f}
                  /\ UNCHANGED <<known, covered, proofValid, suppressed, suppressedForks>>

AcceptCheckpoint(c) == /\ c \in Primary
                       /\ {e \in Primary : e <= c} \subseteq known
                       /\ (covered = NoCheckpoint \/ c >= covered)
                       /\ covered' = c
                       /\ proofValid' = TRUE
                       /\ UNCHANGED <<known, knownForks, suppressed, suppressedForks>>

Suppress(e) == /\ e \in known \cap Covered
               /\ proofValid
               /\ suppressed' = suppressed \cup {e}
               /\ UNCHANGED <<known, knownForks, covered, proofValid, suppressedForks>>

Next == (\E e \in Primary : Receive(e))
     \/ (\E f \in Forks : ReceiveFork(f))
     \/ (\E c \in Primary : AcceptCheckpoint(c))
     \/ (\E e \in Primary : Suppress(e))

TypeOK == /\ known \subseteq Primary
          /\ knownForks \subseteq Forks
          /\ covered \in Primary \cup {NoCheckpoint}
          /\ suppressed \subseteq Primary
          /\ suppressedForks \subseteq Forks

SuppressionRequiresProof == suppressed = {} \/ proofValid
SuppressionIsCovered == suppressed \subseteq known \cap Covered
UnseenForkNeverSuppressed == suppressedForks = {}

Spec == Init /\ [][Next]_vars

=============================================================================
