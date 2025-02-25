module GameServer.AppSpec where

import Control.Concurrent.STM.TVar (newTVarIO)
import qualified Data.Aeson as A
import Data.ByteString (ByteString, isInfixOf)
import Data.ByteString.Lazy (toStrict)
import Data.Functor (($>))
import Data.Maybe (fromJust)
import Data.Text (Text, pack)
import qualified Data.Text as Text
import Data.Text.Encoding (decodeUtf8, encodeUtf8)
import GameServer.App (GameType (Acquire, Bautzen1945), PlayerState (PlayerState), initialState, runApp)
import GameServer.Builder (
    aPlayer,
    anEmptyGame,
    anotherEmptyGame,
    anotherPlayer,
    postJSON,
    putJSON,
    someBackends,
    testSeed,
 )
import GameServer.Log (fakeLogger)
import GameServer.Player as P (Player (playerName), PlayerKey (..), PlayerName (..))
import GameServer.Utils (Id (unId), randomId, (</>))
import Network.HTTP.Types (status404)
import Network.Wai (Application, responseLBS)
import Network.Wai.Test as W (SResponse (simpleBody, simpleHeaders))
import System.Random ()
import Test.Hspec as H (Spec, SpecWith, describe, it)
import Test.Hspec.QuickCheck (prop)
import Test.Hspec.Wai as W (
    MatchBody (MatchBody),
    MatchHeader (..),
    ResponseMatcher (ResponseMatcher),
    WaiSession,
    get,
    shouldRespondWith,
    with,
    (<:>),
 )
import Test.Hspec.Wai.Internal (WaiSession (WaiSession))
import Test.Hspec.Wai.Matcher as W (bodyEquals)
import Test.Hspec.Wai.QuickCheck (Testable (State), property)
import Test.QuickCheck (Arbitrary (arbitrary), elements, forAll)
import Test.QuickCheck.Monadic (monadic, monadicIO)

mkGameServerApp :: IO Application
mkGameServerApp = do
    state <- newTVarIO $ initialState testSeed
    pure $ runApp fakeLogger state someBackends (\_ resp -> resp $ responseLBS status404 [] "Nothing here")

spec :: Spec
spec =
    with mkGameServerApp $
        describe "GameServer Application" $ do
            describe "Games" $ do
                it "on GET /games returns empty list given no game is started" $
                    get "/games" `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode ([] :: [Int]))

                it "on GET /games returns game given one game is started" $ do
                    let game = anEmptyGame
                    postJSON "/games" game

                    get "/games" `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode [game])

                it "on POST /games returns 201 given input game is valid" $
                    postJSON "/games" anEmptyGame
                        `shouldRespondWith` 201

                it "on POST /games returns game's id in Location header given input game is valid" $
                    postJSON "/games" anEmptyGame
                        `shouldRespondWith` ResponseMatcher 201 ["Location" <:> encodeUtf8 ("/games" </> unId (randomId testSeed))] ""

                it "on POST /games ensures returned game id is unique" $ do
                    location <- extractLocation <$> postJSON "/games" anEmptyGame

                    postJSON "/games" anotherEmptyGame
                        `shouldRespondWith` ResponseMatcher 201 (locationDifferentFrom location) ""

                it "on GET /games/<gameId> returns created game started" $ do
                    gameId <- extractGameId <$> postJSON "/games" anEmptyGame

                    get (encodeUtf8 $ "/games" </> gameId)
                        `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode anEmptyGame)
            describe "Players" $ do
                it "on POST /players returns 201 given input player is valid" $
                    postJSON "/players" aPlayer
                        `shouldRespondWith` ResponseMatcher 201 ["Location" <:> encodeUtf8 ("/players" </> playerName aPlayer)] ""

                it "on GET /players returns list of registered players" $ do
                    postJSON "/players" aPlayer

                    get "/players"
                        `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode [aPlayer])
            describe "Players & Games" $ do
                it "on PUT /games/<id>/players returns 200 and player's key given player can join game" $ do
                    postJSON "/players" aPlayer
                    gameId <- newGame

                    putJSON (encodeUtf8 $ "/games" </> gameId </> "players") (PlayerName "Alice")
                        `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode $ PlayerKey "THTBUKBS")

                it "on PUT /games/<id>/players returns 400 given player already joined game" $ do
                    postJSON "/players" aPlayer
                    gameId <- newGame

                    "Alice" `joinsGame` gameId

                    "Alice" `joinsGame` gameId
                        `shouldRespondWith` 400

                it "on PUT /games/<id>/players returns 400 given player is not registered" $ do
                    gameId <- newGame

                    "Alice" `joinsGame` gameId
                        `shouldRespondWith` 400

                it "on GET /games/<id>/players/<playerKey> returns 200 with player info given game is not full" $ do
                    postJSON "/players" aPlayer
                    gameId <- newGame

                    PlayerKey{playerKey} <- fromJust . A.decode . W.simpleBody <$> "Alice" `joinsGame` gameId

                    get (encodeUtf8 $ "/games" </> gameId </> "players" </> unId playerKey)
                        `shouldRespondWith` ResponseMatcher 200 [] (W.bodyEquals $ A.encode $ PlayerState playerKey "Alice")

                it "on GET /games/<id>/players/<playerKey> returns 303 with location to actual game given game is full" $ do
                    postJSON "/players" aPlayer
                    postJSON "/players" anotherPlayer
                    gameId <- newGame

                    aliceKey <- P.playerKey . fromJust . A.decode . W.simpleBody <$> "Alice" `joinsGame` gameId
                    bobKey <- P.playerKey . fromJust . A.decode . W.simpleBody <$> "Bob" `joinsGame` gameId

                    get (encodeUtf8 $ "/games" </> gameId </> "players" </> unId aliceKey)
                        `shouldRespondWith` ResponseMatcher
                            303
                            ["Location" <:> encodeUtf8 ("/games/Bautzen1945" </> gameId </> "players" </> unId aliceKey)]
                            (W.bodyEquals $ A.encode $ PlayerState aliceKey "Alice")

                propW "on GET /games/<game type>/<id>/players/<playerKey> returns index.html for any game/player" $ \(gameType :: GameType) gameId playerKey ->
                    get (encodeUtf8 $ "/games" </> pack (show gameType) </> unId gameId </> "players" </> unId playerKey)
                        `shouldRespondWith` ResponseMatcher
                            200
                            []
                            ( W.MatchBody $ \_ body ->
                                if encodeUtf8 (pack (show gameType)) `isInfixOf` toStrict body
                                    then Nothing
                                    else Just ("invalid content returned: " <> show body)
                            )

                propW "on GET /games/<game type>/<id>/players/<playerKey>/whatever returns 'whatever'" $ \(gameType :: GameType) gameId playerKey extension ->
                    get (encodeUtf8 $ "/games" </> pack (show gameType) </> unId gameId </> "players" </> unId playerKey </> selectExtension gameType extension)
                        `shouldRespondWith` ResponseMatcher
                            200
                            []
                            ( W.MatchBody $ \_ body ->
                                if marker extension `isInfixOf` toStrict body
                                    then Nothing
                                    else Just ("invalid content returned: " <> show body)
                            )

marker :: Extension -> ByteString
marker SomeJs = "function"
marker SomeCSS = "width:"
marker SomeImg = "PNG"

selectExtension :: GameType -> Extension -> Text
selectExtension Bautzen1945 SomeJs = "bautzen1945.js"
selectExtension Bautzen1945 SomeCSS = "bautzen1945.css"
selectExtension Bautzen1945 SomeImg = "assets/german/17-hq-recto.png"
selectExtension Acquire SomeJs = "acquire.js"
selectExtension Acquire SomeCSS = "acquire.css"
selectExtension Acquire SomeImg = "images/ui-player.png"

data Extension
    = SomeJs
    | SomeCSS
    | SomeImg
    deriving (Eq, Show)

instance Arbitrary Extension where
    arbitrary = elements [SomeJs, SomeImg, SomeCSS]

propW :: Testable prop => String -> prop -> SpecWith (State prop, Application)
propW s p = it s $ property p

joinsGame :: Text -> Text -> WaiSession () SResponse
joinsGame pName gameId = putJSON (encodeUtf8 $ "/games" </> gameId </> "players") (PlayerName pName)

newGame :: WaiSession () Text
newGame = extractGameId <$> postJSON "/games" anEmptyGame

locationDifferentFrom :: ByteString -> [MatchHeader]
locationDifferentFrom loc =
    [ W.MatchHeader $
        \headers _ -> case lookup "Location" headers of
            Nothing -> Just "No Location header"
            Just h | h == loc -> Just ("expected 'Location' header to differ from " <> show loc)
            Just h -> Nothing
    ]

extractGameId = Text.drop 7 . decodeUtf8 . extractLocation

extractLocation = fromJust . lookup "Location" . W.simpleHeaders
