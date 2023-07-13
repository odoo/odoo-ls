# Odoo LSP, comment ça marche?

## A - Symbols

Un symbole représente tout élément venant du code python: variable, fichier, fonctions, classes, etc...
Un symbole peut donc être de plusieurs types:
- NAMESPACE: un dossier
- PACKAGE: un dossier contenant un fichier `__init__.py`
- FILE: un fichier
- COMPILED: une ressource compilée
- CLASS: une classe
- VARIABLE: une variable
- FUNCTION: une fonction
- PRIMITIVE: une évaluation d'un type primitif

Les symboles entre eux forment un graphe, de la manière suivante:
  - Un symbole a un et un seul parent.
  - Un symbole a trois listes d'enfants:
    - les `symbols`: la liste des sous-symboles exposés provenant d'un fichier python
    - les `modulesSymbols`: la liste des sous-symbols provenant de la structure des fichiers sur le disque
    - les `localSymbols`: les candidats à la liste 'symbols' qui se sont fait rejetés car écrasés par un autre du même nom, conservés pour une évaluation locale potentielle

Exemple:

Prenons la structure de fichier suivante:

```
addons (dir)
    | module (dir)
        | __init__.py (file)
        | file.py (file)
```

`__init__.py` contient

```python
from . import file
```

et `file.py` contient

```python
import os
from odoo import models

variable = 5
print(variable)
variable = 6

class Test(models.Model):

    def func(self):
        return None
```

Nous obtiendrons les symboles suivants:

- `root`
  - type: ROOT
  - path: [sys.path, odoo_addons_path, config]
  - parent: None
  - symbols: []
  - moduleSymbols: `addons`
  - localSymbols: []
- `addons`
  - type: NAMESPACE
  - path: path_to_project + `addons`
  - parent: `root`
  - symbols: []
  - moduleSymbols: `module`
  - localSymbols: []
- `module`
  - type: PACKAGE
  - path: path_to_project + `addons/module`
  - parent: `addons`
  - symbols: [`file (A)`]
  - moduleSymbols: `file (B)`
  - localSymbols: []
`file (A)` et `file (B)`réfèrent à deux `file` différents:
- `file (A)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/__init__.py`
  - parent: `module`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `file (B)`
  - type: FILE
  - path: path_to_project + `addons/module/file.py`
  - parent: `module`
  - symbols: [`os`, `models`, `variable (B)`, `Test`]
  - moduleSymbols: []
  - localSymbols: [`variable (A)`]
- `os`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `models`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `variable (A)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `variable (B)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `Test`
  - type: CLASS
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: [`func`]
  - moduleSymbols: []
  - localSymbols: []
- `func`
  - type: FUNCTION
  - path: path_to_project + `addons/module/file.py`
  - parent: `Test`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []

### Référencement

Un élément de l'arbre peut être identifié par une "clé" unique, représentant le chemin à suivre depuis l'élément root pour atteindre le symbole.
Ce "chemin" est un tuple de deux listes.
La première liste contient tous les éléments 'génériques', et la deuxième les éléments propres à un fichier.
Exemple basé sur la structure utilisée précédement:
```python
(["addons", "module", "file"], ["Test", "func"])
```
La particularité de cette double liste est quelle permet de résoudre une ambiguité dans l'arbre: les fichiers versus les variables.
Dans un chemin avec une seule liste, le symbole 
```python
["addons", "module", "file"]
```
est ambigü. En effet, est-ce que `file` fait référence au fichier `file.py` ou à la variable `file` contenue dans `module`.
La double liste permet de casser cette ambiguité:
```python
(["addons", "module", "file"], [])
```
représente le fichier, tandis que 
```python
(["addons", "module"], ["file"])
```
représente la variable.
En effet, la première liste représente la structure sur le disque (`moduleSymbols`), et la seconde les éléments contenus dans un fichier (`symbols`), avec en fallback les symbols du disque (`moduleSymbols`).
Toutefois, sans ambiguïté dans l'arbre, le chemin complet peut se mettre dans la première liste.


## B - Evaluation

Un symbole contient une Evaluation. Il s'agit de la valeur à laquelle le symbole est évalué, si c'est possible.
Evidement, pas d'évaluation pour les FILE, NAMESPACE, PACKAGE.
les FUNCTION ont une Evaluation de leur valeur retour.

## C - Memory management

Python c'est bien. Mais la gestion de la mémoire peut vite devenir chaotique, et la structure du code (graphe et référence vers des symboles dans les evaluations et autres caches) rend l'update (suppression, invalidation) de mémoire très difficile. En particulier, la suppression d'un élément de l'arbre des symboles n'invalide pas toutes les références à cet élément dans tous les autres éléments du code (evaluation et caches). Pour palier à ça, il est possible soit:
  - d'utiliser la notation (voir plus haut) permettant de retrouver un élément dans l'arbre et ne jamais le référencer directement (mais cela implique beaucoup de recherches dans l'arbre, et un stockage long de chaine de charactère représentant le chemin dans l'arbre)
  - d'utiliser des pointeurs faibles. ([weakref](https://docs.python.org/3/library/weakref.html))
  - d'utiliser des références à double sens: une référence peut être supprimée par l'objet qu'elle référence

La première solution a été rejetée par sa lourdeur à l'exécution et par la difficulté de debugging de l'application, aucune référence n'étant résolue lors d'un breakpoint

La deuxième solution, d'abord implémentée à titre d'essai a vite montré ses limites lors d'une utilisation régulière. En effet, un weakref ne devient invalide que lorsque le garbage collector a effectivement détruit un objet.
Il est donc parfois possible de réssusciter un objet supprimé:
```python
a = 5
ref = weakref.ref(a)
del a
old_a = ref() #if the garbage collector did not collect a already, old_a will be a strong ref to a
```

La troisième solution a alors été choisie et implémentée dans le fichier references.py afin de remplacer weakref. Le principe est de proposer la même interface que weakref, mais de ne pas se baser sur le garbage collector. Lorsqu'un objet est supprimé, il appelle toutes les références qui le pointent et signale sa suppression immédiatement.

Le code est donc organisé comme suit, et ce principe doit être une règle SAINTE pour tout développement dans le projet:
Tous les symboles ne sont référencés de manière forte qu'*UNE SEULE FOIS*, par leur symbole parent. TOUTE AUTRE référence à un symbole depuis un autre endroit du code, ou meme entre symbole doit se faire via des references faibles (RegisteredRef).
Ainsi la suppression d'un symbole est immédiate et effective.

### Accès asynchrone

Ceci amène tout doucement à un autre problème: lorsque du code prend une référence sur un weakref et donc un symbole, le nombre de référence passe à 2 (ou plus), et la suppression d'un symbole n'est plus immédiate.
Plus généralement, on aimerait assurer une consistance des données à travers plusieurs threads.
Ceci est effectué par 3 méthodes:
`Odoo.acquire_write()`, `Odoo.acquire_read()` et `Odoo.upgrade_to_write()`

Avant tout accès aux données du LSP, il convient de demander l'accès correspondant au besoin, et de s'y tenir. Si un accès en lecture a besoin d'être élevé en accès en écriture, `Odoo.upgrade_to_write()` peut être utilisé. Il est garanti que seul un accès en écriture peut avoir lieu simulatément, mais il peut y avoir autant de lecteur qu'on veut. Aucun accès en écriture et lecture ne peut être simultané.
Demander un accès en écriture revient à rendre le plugin monothread pour la durée du lock.

## D - Construction de la base de données

L'arbre de symbol doit bien entendu être construit et maintenu à jour. Ce processus est fait de la manière suivante:

- A) construire l'architecture 
- B) Evaluation de l'architecture
- C) Initialiser les objets Odoo
- D) répéter A et B pour base, puis les modules
- E) Valider le code

### Construire l'architecture

Cette étape prépare l'arbre de base en construisant tous les symboles détectés dans les fichiers. Cette partie charge tous les fichiers python en suivant les différents imports (+ les dossiers `tests` dans les modules) et construit l'arbre correspondant.

*Note: Les librairies externes sont aussi parsées si trouvées. Si pas, le code cherche un stubs existant dans le repo `typeshed` embarqué. Toutefois, afin de réduire la mémoire utilisée, l'arbre est figé une fois parsé, aucun changement ne peut y être apporté par la suite et les caches sont figés. (TODO: généraliser à tout dossier hors workspace)*

### Evaluation de l'architecture

Cette partie contient l'évaluation des symboles. Si possible, le code va essayer d'évaluer et associer la valeur trouvée au symbole.
Cette évaluation concerne toutes les variables à la racine d'un fichier ou d'une classe uniquement. Les symboles sous une fonction ne sont pas évalués à cette étape, car ils ont souvent besoin d'avoir la structure Odoo de construite. Néanmoins, si une doc existe pour la fonction, la valeur de retour peut déjà être évaluée (TODO ?)

### Initialiser les objets Odoo

Cette étape repasse sur les symboles précédement créés pour reconstruire la structure d'Odoo: models, modules, etc... sont regroupés et cachés pour un accès plus rapide par la suite.
Le stockage des données de référence RESTE l'arbre de symboles.
Toutefois, la classe Odoo contient des objets avec des références vers cet arbre:
- `models`: dictionnaire, qui pour chaque nom de modèle (ex: `"mail.thread"`), associe un objet Model. Cet objet recueille l'ensemble des symboles représentant un modèle, et permet de répondre à des requêtes sur un modèle auxquelles il va répondre en utilisant ces symboles.
- `modules`: regroupe l'ensemble des symboles représentant un module Odoo, ainsi que les informations du manifest qui y est lié.

Cette étape permet aussi de créer les symboles qui échappent à la première étape de par leur nature dynamique: `self.env`, `env.cr`, etc...

### Validation

Cette dernière étape ne construit pas la base de données, mais fait un dernier passage dessus afin de pouvoir en générer des diagnostiques maintenant que la base de données est complète.
C'est l'étape qui rajoute tous les infos/warnings/erreurs dans le projet.

## E - Mise à jour et dépendences

~~La drogue c'est mal.~~ Une fois la base de données construite, il s'agit évidement de la garder à jour.
Pour chaque mise à jour du code, il convient donc de refléter les changements dans les structures de symboles.

Un première possibilité serait d'analyser le changement effectué et de changer localement les symboles concernés. Toutefois cette solution implique énormément de code et de complexité pour réussir à isoler ces changements et garder une consistance dans l'arbre final.

L'autre solution est de reconstruire l'entiereté d'un fichier lors d'une modification, quelle qu'elle soit. En réalité, lorsqu'un fichier est chargé en mémoire, l'analyser est très rapide, et le cout de reconstruire l'entiereté du fichier au lieu du symbole concerné est beaucoup trop faible pour justifier un code beaucoup plus lourd.

Une fois le fichier reconstruit, il convient de s'assurer que tous les éléments autres que ces symboles soient aussi mis à jour. Que ce soit les Evaluations pointant vers ces symboles ou d'autres parties de l'arbre dont la construction en dépendait.
Pour cela, lors de la construction/evaluation/validation d'un symbole, si un autre entre dans le processus, le premier symbole est noté comme "dépendant de" sur le second symbole.
Lors de la reconstruction d'un fichier, tous les symbols supprimés marques ajoutent leurs dépendences (inversées) dans une liste de symboles à reconstruire/évaluer/valider. Ceux-ci feront potentiellement de même avec leurs dépendences. 

Note: Dans le cas d'un code erroné, il peut arriver qu'une évaluation échoue car le symbole qui est censé être pointé n'existe pas. Dans ce cas la dépendence ne peut être créé et aucune résolution d'erreur ne peut se faire si le symbole venait à apparaitre. Dès lors, tous les symboles manquants sont enregistrés dans la classe Odoo, et tout nouveau symbole vérifie dans cette liste s'il ne résout pas une erreur existante.

### types de dépendences

Lorsqu'un symbole enregistre sa dépendence auprès d'un autre symbole, il indique le type de dépendence:
- __ARCH__: la construction du symbole dépend du second symbole. Un exemple typique serait `import *`. cette instruction va créer toute une série de symboles qui dépendent du fichier importé. Si le fichier importé est modifié, alors les symboles ajouté par `import *` doivent être relistés
- __ARCH_EVAL__: L'architecture des symboles ne dépend pas du second symbole, mais son évaluation bien. Cette dépendence est différente de la première, car la liste de symbole n'est pas modifiée. Elle permet de ne pas propager une modification architecturalle trop loin.
- __ODOO__: les éléments propres à Odoo ont été modifiés (_inherit, _name, base class, etc...)
- __VALIDATION__: La validation indique qu'une erreur pourrait survenir si le second symbole est modifié.

## F - comme Features

Le language serveur, avec sa base de données et ses notifications, peut fournir les fonctionnalités suivantes:

- __Diagnostique__: Donne une liste d'indication sur le code analysé. Ces diagnostiques peuvent être trouvés dans l'onglet "problèmes" sur vscode, et visibles généralement dans le code via un soulignement bleu/jaune/rouge.
- __Hover__: Donne une bulle d'information a afficher concernant l'élément se trouvant actuellement sous le curseur de la souris
- __Go To Definition__: Permet de se rendre directement à l'emplacement de la déclaration du symbole sous le curseur.
- __Autocomplétion__: fournit une liste de candidats pour compléter le code sous le curseur
- __Refactoring (A venir)__: Aide au refactoring avec "replace all". 

## G - Requêtes d'évaluation

Afin de pouvoir répondre aux requêtes d'autocomplétion, aux Hover et aux goto definition, parsoUtils.evaluateType est une fonction capable d'évaluer le type d'un morceau de code donné en paramètre.
C'est l'interface principale pour interagir avec la base de connaissance.

Cette fonction utilise un context pour transférer les informations importantes d'étape en étape. Ce contexte contient trois clés: args, parent et module.

Dans le cas de `self.env["test"].func(a)`
- le context lors de l'évaluation de self sera `{module: currentModule}`
- le context lors de l'évaluation de env sera `{args: None, parent: self, module: currentModule}`
- le context lors de l'évaluation de env.__getitem__ sera `{args: "test", parent: env, module: currentModule}`
- le context lors de l'évaluation de func() sera `{args: {a}, parent: TestModel, module: currentModule}`