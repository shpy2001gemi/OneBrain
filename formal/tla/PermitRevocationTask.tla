---------------------- MODULE PermitRevocationTask -------------------------
EXTENDS Naturals

VARIABLES accepted, revokedRelative, exactScope, executed, executionAuthorizedAtTime
vars == <<accepted, revokedRelative, exactScope, executed, executionAuthorizedAtTime>>

Init == /\ accepted = FALSE
        /\ revokedRelative = FALSE
        /\ exactScope = FALSE
        /\ executed = FALSE
        /\ executionAuthorizedAtTime = FALSE

Accept == /\ ~accepted
          /\ accepted' = TRUE
          /\ UNCHANGED <<revokedRelative, exactScope, executed, executionAuthorizedAtTime>>

ObserveRevocation == /\ accepted
                     /\ ~revokedRelative
                     /\ revokedRelative' = TRUE
                     /\ UNCHANGED <<accepted, exactScope, executed, executionAuthorizedAtTime>>

BindExactScope == /\ ~exactScope
                  /\ exactScope' = TRUE
                  /\ UNCHANGED <<accepted, revokedRelative, executed, executionAuthorizedAtTime>>

Execute == /\ accepted
           /\ ~revokedRelative
           /\ exactScope
           /\ ~executed
           /\ executed' = TRUE
           /\ executionAuthorizedAtTime' = TRUE
           /\ UNCHANGED <<accepted, revokedRelative, exactScope>>

Next == Accept \/ ObserveRevocation \/ BindExactScope \/ Execute
Spec == Init /\ [][Next]_vars

ExecutionAuthorized == executed => executionAuthorizedAtTime

=============================================================================
