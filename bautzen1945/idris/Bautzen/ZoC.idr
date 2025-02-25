module Bautzen.ZoC

import Bautzen.GameUnit
import Bautzen.Terrain

import public Data.List
import public Data.List.Elem
import public Decidable.Equality
import Data.Nat
import Data.Vect


-- section 3
-- zones of control

public export
data ZoC : Type where
  InZoC : (side : Side) -> ZoC
  Free : ZoC

public export
Eq ZoC where
  Free == Free = True
  (InZoC s) == (InZoC s') = s == s'
  _ == _ = False

||| Test if given position for given `side` is in the ZoC of the unit.
public export
inZocOf : (pos : Pos) -> (side : Side) -> (GameUnit, Pos) -> Bool
inZocOf pos curSide (unit, location) =
  if curSide == side (nation unit)
  then False
  else case pos `isElem` neighbours location of
    (Yes prf) => True
    (No contra) => False

||| Is the given `Pos`ition in an enemy ZoC?
||| This assumes the current `side` is playing and checking ZoCs
public export
inZoC : Side -> List (GameUnit, Pos) -> Pos -> ZoC
inZoC curSide units pos =
  case find (inZocOf pos curSide) units of
    Nothing => Free
    (Just (unit, _)) => InZoC (side $ nation unit)

-- ZoC tests

-- inZoCTrue : inZocOf (hex 3 3) Axis (Bautzen.GameUnit.p13_5dp, hex 3 4) = True
-- inZoCTrue = Refl

-- inZoCTrue2 : inZocOf (hex 4 3) Axis (Bautzen.GameUnit.p13_5dp, hex 3 4) = False
-- inZoCTrue2 = Refl

inZoCFalsePolish : inZocOf (hex 3 3) Allies (Bautzen.GameUnit.p13_5dp, hex 3 4) = False
inZoCFalsePolish = Refl
