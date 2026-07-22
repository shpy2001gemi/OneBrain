--------------------------- MODULE ProviderLease ---------------------------
EXTENDS Naturals, FiniteSets

Generations == 1..3
VARIABLES maxGeneration, retirementFloor, highWaterRecords
vars == <<maxGeneration, retirementFloor, highWaterRecords>>

Init == /\ maxGeneration = 0
        /\ retirementFloor = 0
        /\ highWaterRecords = {}

ObserveLease(g, id) ==
    /\ g \in Generations
    /\ id \in 1..2
    /\ IF g > maxGeneration
          THEN /\ maxGeneration' = g
               /\ highWaterRecords' = {id}
          ELSE IF g = maxGeneration
               THEN /\ maxGeneration' = maxGeneration
                    /\ highWaterRecords' = highWaterRecords \cup {id}
               ELSE /\ maxGeneration' = maxGeneration
                    /\ highWaterRecords' = highWaterRecords
    /\ UNCHANGED retirementFloor

RetireThrough(g) == /\ g \in Generations
                    /\ retirementFloor' = IF g > retirementFloor THEN g ELSE retirementFloor
                    /\ UNCHANGED <<maxGeneration, highWaterRecords>>

Next == (\E g \in Generations, id \in 1..2 : ObserveLease(g, id))
     \/ (\E g \in Generations : RetireThrough(g))

Spec == Init /\ [][Next]_vars
NoResurrection == (maxGeneration <= retirementFloor) => ~(maxGeneration > retirementFloor)
ConflictsRetained == Cardinality(highWaterRecords) <= 2

=============================================================================
