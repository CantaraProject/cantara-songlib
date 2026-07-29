\version "2.24.0"

\header {
  title = "Sei nicht stolz auf das, was du bist"
  composer = "Jan Martin Reckel"
  tagline = ##f
}
\paper {
  #(set-paper-size "a4")
}

\layout {
  indent = #0
  \context {
    \Voice
    \consists "Melody_engraver"
    \override Stem.neutral-direction = #'()
  }
}

global = {
  \key d \major
  \time 4/4
  \partial 4
}

sopranoVoiceStanza = \relative c' {
  \global
  d8 e | fis4 fis g4 fis8 e | a2. \breathe
  d,8 e | fis4 fis g4 fis8 e | e2. \breathe
  d8 e | fis4 fis g  fis8 g | a4 d b \breathe
  d,8 e | fis4 fis4 g4 fis8 e | d2.
}

sopranoVoiceRefrain = \relative c' {
  fis8( g ) | a8 a a a d,4. d8 | b' a g fis e4 d8( e ) |
  fis4 fis g4 fis8( e ) | e2. r4
  a8 a a a d,4. d8 | b' a g fis e4 d8( e ) |
  fis4 fis fis8 a g fis | e4 e d4 \bar "|."
}

verseOne = \lyricmode {
  \set stanza = "1."
  Sei nicht stolz auf das, was du bist,
  denn nur Gott gut und hei -- lig ist.
  Su -- che ihn, so lang er sich fin -- den lässt,
  dann kommt Hoff -- nung, Frie -- de und Freud.
  
  Denn wer sich rüh -- men will,
  der rüh -- me sich des Herrn;
  denn er ist uns nicht fern.
  Wer nun an ihn glaubt,
  und fest auf ihn ver -- traut,
  der hat sein Le -- ben nicht auf
  Sand ge -- baut.
}

verseTwo = \lyricmode {
  \set stanza = "2."
  Men -- schen su -- chen Glück bei sich selbst,
  doch ein Mensch al -- lein sich nicht hält.
  Chris -- tus Je -- sus kam ja auf die -- se Welt,
  um zu ge -- ben, was uns noch fehlt.
}

verseThree = \lyricmode {
  \set stanza = "3."
  Sün -- der, sie doch dei -- ne Schuld an,
  Je -- sus hat sie weg -- ge -- tan. _
  Je -- dem, der nun um -- kehrt und an ihn glaubt,
  tut sich heut die Him -- mels -- tür auf.
  \set ignoreMelismata = ##t Da -- rum \unset ignoreMelismata
}

sopranoVoicePart = \new Staff \with {
  midiInstrument = "choir aahs"
} { \sopranoVoiceStanza \sopranoVoiceRefrain }
\addlyrics { \verseOne }
\addlyrics { \verseTwo }
\addlyrics { \verseThree }

\score {
  <<
    \sopranoVoicePart
  >>
  \layout { }
  \midi { }
}

