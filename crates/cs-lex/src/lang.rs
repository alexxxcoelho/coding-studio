//! Languages of the lexical workbench: stop words, Snowball stemming and
//! accent folding. French is the default, English and German are supported.

use std::collections::{BTreeSet, HashMap};

use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};

/// A supported language. Serialised as its two-letter code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "de")]
    De,
}

impl Lang {
    /// `fr`, `en` or `de` (case-insensitive, region suffix ignored: `fr-CH`).
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_lowercase();
        let base = s.split(['-', '_']).next().unwrap_or("");
        match base {
            "fr" | "french" | "francais" | "français" => Some(Lang::Fr),
            "en" | "english" | "anglais" => Some(Lang::En),
            "de" | "german" | "allemand" | "deutsch" => Some(Lang::De),
            _ => None,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    fn algorithm(&self) -> Algorithm {
        match self {
            Lang::Fr => Algorithm::French,
            Lang::En => Algorithm::English,
            Lang::De => Algorithm::German,
        }
    }

    /// Stop words of the language, accent-folded.
    pub fn stop_words(&self) -> &'static BTreeSet<String> {
        use std::sync::OnceLock;
        static FR: OnceLock<BTreeSet<String>> = OnceLock::new();
        static EN: OnceLock<BTreeSet<String>> = OnceLock::new();
        static DE: OnceLock<BTreeSet<String>> = OnceLock::new();
        let (cell, list) = match self {
            Lang::Fr => (&FR, STOP_FR),
            Lang::En => (&EN, STOP_EN),
            Lang::De => (&DE, STOP_DE),
        };
        cell.get_or_init(|| list.split_whitespace().map(fold).collect())
    }

    pub fn is_stop(&self, folded: &str) -> bool {
        self.stop_words().contains(folded)
    }
}

/// Lower-case and remove the diacritics of the Latin letters, so that
/// "Économie" and "economie" meet. Apostrophes are normalised to `'`.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for l in c.to_lowercase() {
            match l {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
                'ç' => out.push('c'),
                'è' | 'é' | 'ê' | 'ë' => out.push('e'),
                'ì' | 'í' | 'î' | 'ï' => out.push('i'),
                'ñ' => out.push('n'),
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
                'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
                'ý' | 'ÿ' => out.push('y'),
                'œ' => out.push_str("oe"),
                'æ' => out.push_str("ae"),
                'ß' => out.push_str("ss"),
                '\u{2019}' | '\u{2018}' | '\u{02bc}' => out.push('\''),
                other => out.push(other),
            }
        }
    }
    out
}

/// A Snowball stemmer per language, with a cache per distinct word.
pub struct Stemmers {
    fr: Stemmer,
    en: Stemmer,
    de: Stemmer,
    cache: HashMap<(Lang, String), String>,
}

impl Default for Stemmers {
    fn default() -> Self {
        Stemmers {
            fr: Stemmer::create(Lang::Fr.algorithm()),
            en: Stemmer::create(Lang::En.algorithm()),
            de: Stemmer::create(Lang::De.algorithm()),
            cache: HashMap::new(),
        }
    }
}

impl Stemmers {
    /// Stem of a lower-case word (accents kept: the Snowball algorithms of
    /// French and German expect them). Apostrophes are removed first.
    pub fn stem(&mut self, lang: Lang, lower: &str) -> String {
        if let Some(s) = self.cache.get(&(lang, lower.to_string())) {
            return s.clone();
        }
        let clean: String = lower.chars().filter(|c| *c != '\'').collect();
        let stemmer = match lang {
            Lang::Fr => &self.fr,
            Lang::En => &self.en,
            Lang::De => &self.de,
        };
        let s = stemmer.stem(&clean).into_owned();
        self.cache.insert((lang, lower.to_string()), s.clone());
        s
    }
}

const STOP_FR: &str = "
a à au aux avec ce ces cet cette ceci cela celà celui celle ceux celles ça c ç
d dans de des du donc dont
elle elles en encore entre et eux
il ils j je jusqu jusque
la le les leur leurs lui l
ma mais me même mêmes mes moi mon m
n ne ni nos notre nous
on ou où
par parce pas pour pourquoi
qu quand que quel quelle quelles quels qui quoi
s sa sans se ses si sien son sont sous sur
t ta te tes toi ton tu tout tous toute toutes
un une unes uns
vos votre vous
y
ai as avons avez ont avais avait avions aviez avaient aurai auras aura aurons aurez auront aurais aurait
ayant eu eue eus eut eût
suis es est sommes êtes sont étais était étions étiez étaient serai sera serons serez seront serait
été étant être avoir
fais fait faisons faites font faisait ferai fera feront ferait faire
peux peut pouvons pouvez peuvent pouvait pourrait pouvoir
vais vas va allons allez vont allait aller
faut fallait falloir
alors ainsi après avant aussi autre autres bien car chez comme comment depuis déjà dès encore enfin
ici lors lorsque maintenant moins plus puis quelque quelques sinon soit tant tel tels très trop vers
voici voilà
oui non peu beaucoup toujours jamais souvent parfois vraiment quand
";

const STOP_EN: &str = "
a about above after again against all am an and any are aren't as at
be because been before being below between both but by
can can't cannot could couldn't
did didn't do does doesn't doing don't down during
each few for from further
had hadn't has hasn't have haven't having he he'd he'll he's her here here's hers herself him himself his how how's
i i'd i'll i'm i've if in into is isn't it it's its itself
let's me more most mustn't my myself
no nor not of off on once only or other ought our ours ourselves out over own
same shan't she she'd she'll she's should shouldn't so some such
than that that's the their theirs them themselves then there there's these they they'd they'll they're they've this those through to too
under until up very
was wasn't we we'd we'll we're we've were weren't what what's when when's where where's which while who who's whom why why's with won't would wouldn't
you you'd you'll you're you've your yours yourself yourselves
also just like get got yes
";

const STOP_DE: &str = "
aber alle allem allen aller alles als also am an ander andere anderem anderen anderer anderes auch auf aus
bei bin bis bist da damit dann der den des dem die das dass daß dein deine deinem deinen deiner dich dir dies diese diesem diesen dieser dieses doch dort du durch
ein eine einem einen einer eines einig einige er es etwas euch euer eure
für gegen gewesen hab habe haben hat hatte hatten hier hin hinter ich ihm ihn ihnen ihr ihre ihrem ihren ihrer im in indem ins ist
jede jedem jeden jeder jedes jetzt kann kein keine keinem keinen keiner können könnte machen man manche manchem manchen mancher manches mein meine meinem meinen meiner mich mir mit muss musste
nach nicht nichts noch nun nur ob oder ohne sehr sein seine seinem seinen seiner seit sich sie sind so solche solchen solcher solches soll sollte sondern sonst über um und uns unser unsere unserem unseren unter viel vom von vor während war waren warst was weg weil weiter welche welchem welchen welcher welches wenn werde werden wie wieder will wir wird wirst wo wollen wollte würde würden zu zum zur zwar zwischen
";
