---------------------- MODULE ReconciliationSession ------------------------
EXTENDS Naturals

VARIABLES contextBound, rootsEqual, pendingRanges, selectorComplete
vars == <<contextBound, rootsEqual, pendingRanges, selectorComplete>>

Init == /\ contextBound = FALSE
        /\ rootsEqual = FALSE
        /\ pendingRanges = 2
        /\ selectorComplete = FALSE

BindContext == /\ ~contextBound
               /\ contextBound' = TRUE
               /\ UNCHANGED <<rootsEqual, pendingRanges, selectorComplete>>

MatchRoots == /\ ~rootsEqual
              /\ rootsEqual' = TRUE
              /\ UNCHANGED <<contextBound, pendingRanges, selectorComplete>>

DrainRange == /\ pendingRanges > 0
              /\ pendingRanges' = pendingRanges - 1
              /\ UNCHANGED <<contextBound, rootsEqual, selectorComplete>>

Complete == /\ contextBound
            /\ rootsEqual
            /\ pendingRanges = 0
            /\ selectorComplete' = TRUE
            /\ UNCHANGED <<contextBound, rootsEqual, pendingRanges>>

ChangeContext == /\ selectorComplete
                 /\ contextBound' = FALSE
                 /\ selectorComplete' = FALSE
                 /\ UNCHANGED <<rootsEqual, pendingRanges>>

Next == BindContext \/ MatchRoots \/ DrainRange \/ Complete \/ ChangeContext
Spec == Init /\ [][Next]_vars

ScopedCompletion == selectorComplete => (contextBound /\ rootsEqual /\ pendingRanges = 0)

=============================================================================
