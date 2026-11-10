# Discovery: formación y balanceo de grupos según perfiles

Nov 7, 2026 · @Agustin Rios

Existe bastante software e investigación sobre formar y balancear grupos según perfiles de personas; el problema aparece con nombres distintos según la disciplina.

## Vocabulario clave

| Término | Disciplina | Qué busca |
| --- | --- | --- |
| Team Formation Problem (TFP) | General / data mining | Formar equipos según habilidades, preferencias y restricciones |
| [Maximally Diverse Grouping Problem (MDGP)](https://itc.ktu.lt/index.php/ITC/article/view/977) | Investigación de operaciones | Partir un conjunto en un número dado de grupos disjuntos, con tamaños acotados (mínimo y máximo) y lo más diversos posible. Es NP-hard |
| [Anticlustering](https://github.com/m-Py/anticlust/) | Psicometría / estadística | Maximizar la similitud entre grupos y la heterogeneidad dentro de cada uno; lo inverso del clustering |
| Group formation en CSCL | Educación (Computer-Supported Collaborative Learning) | Formar grupos de aprendizaje con criterios pedagógicos |
| Matchmaking / balanceo de equipos | Gaming | Armar partidas con equipos de fuerza similar (Elo, TrueSkill) |

## Software existente

CATME es el referente, pero es cerrado y pagado; las alternativas abiertas están poco mantenidas o son de nicho.

| Herramienta | Tipo / licencia | Cómo agrupa | Fortaleza | Limitación |
| --- | --- | --- | --- | --- |
| [CATME Team-Maker](https://digitalcommons.georgiasouthern.edu/management-facpubs/13) (Purdue) | Web, cerrado, pagado | Encuesta con criterios (horarios, notas, liderazgo) ponderados por el instructor; heurística max-min; tamaño mínimo y máximo | Validado: cumplió los criterios mejor que profesores con experiencia | [Ya no es open source y cobra](https://peer.asee.org/34717.pdf) |
| [gruepr](https://signpath.org/projects/gruepr) (Northeastern) | Escritorio C++/Qt, GPL v3 | [Algoritmo genético multi-hilo](https://coed.asee.org/?p=4162) sobre datos de Google Forms | Gratis, flexible, de 4 a 1000 estudiantes en minutos | App de escritorio, foco educativo |
| [MoodlePeers / GroupAL](https://moodle.org/plugins/pluginversion.php?id=22303) | Plugin de Moodle | Cuestionarios de personalidad, motivación y orientación al equipo; opcional conocimiento previo o temas | [Evidencia empírica](https://fitforthem.unipa.it/rep:6533b820fe1ef96bd127a240) vs. grupos aleatorios: más entregas, mejor calidad y satisfacción | Última versión soporta hasta Moodle 3.10 |
| [anticlust](https://github.com/m-Py/anticlust/) | Paquete R, MIT | Anticlustering: K grupos sobre variables numéricas; `categories` balancea una variable categórica | Limpio conceptualmente; [superó](https://zendy.io/title/10.1037/met0000301) a asignación aleatoria y matching | Librería, sin interfaz |
| [Teambuilder](https://github.com/UTDallasEPICS/Teambuilder/wiki/OR%E2%80%90Tools-Integration) (UT Dallas) | Web Vue + TypeScript + Python | Google OR-Tools CP-SAT: restricciones duras (tamaño, días) y objetivos blandos (preferencias, skills, diversidad) | Buena referencia de arquitectura full-stack | Repo archivado en agosto de 2026 |
| [TrueSkill](https://www.microsoft.com/en-us/research/project/trueskill/) (Microsoft) | Sistema de rating; TrueSkill 2 patentado | Habilidad como media + incertidumbre; [busca partidas con probabilidad de victoria pareja](https://microsoft.com/en-us/research/wp-content/uploads/2006/10/Game-Developer-Feature-Article-Graepel-Herbrich.pdf) | Probado a escala en Xbox | [Solo suma habilidades; ignora dinámica de equipo](https://arxiv.org/pdf/2006.13763v1.pdf) |

## Investigación académica

La investigación se reparte en cuatro líneas, de la más algorítmica a la más aplicada.

- **Algoritmos para MDGP.** Hay mucha literatura de metaheurísticas: [tabu search con oscilación estratégica](https://fitforthem.unipa.it/rep:6533b836fe1ef96bd12a0ba2), [recocido simulado, genéticos y VNS](https://itc.ktu.lt/index.php/ITC/article/view/977) con benchmarks de hasta 2000 elementos. Un [recocido simulado adaptativo (AAAI)](https://ojs.aaai.org/index.php/AAAI/article/view/41006/44967) mejoró el mejor valor conocido en 460 instancias.
- **Equipos con red social.** El paper fundacional es [Lappas et al. (KDD 2009)](https://faculty.cc.gatech.edu/~zha/CSE8801/social-network/p467-lappas.pdf): además de cubrir habilidades, minimiza el costo de comunicación sobre el grafo social.
- **Revisiones en educación (CSCL).** [Maqtary et al. (2019)](https://eprints.glos.ac.uk/4867/) analizó 30 estudios y propuso dos taxonomías: atributos para formar grupos y técnicas.
- **Roles de Belbin con IA.** Una [herramienta iterativa](https://ar5iv.arxiv.org/html/1604.04721) propone equipos, recoge feedback sobre los roles que cada uno asumió y refina las configuraciones siguientes.

## Qué conviene balancear

Más diversidad no siempre es mejor: cada variable pide un tratamiento distinto.

- Un [meta-análisis](https://arxiv.org/pdf/1610.08804) encontró que la diversidad relacionada con la tarea (formación, expertise) se correlaciona positivamente con el desempeño. No encontró una relación clara para la diversidad bio-demográfica.
- En personalidad, [el desempeño mejora](https://pmc.ncbi.nlm.nih.gov/articles/PMC10894934/table/tab2) con un nivel promedio alto de responsabilidad (conscientiousness) y empeora cuando hay mucha diversidad en ese rasgo.
- Los efectos de la personalidad suelen ser más débiles en equipos ad hoc que en equipos estables.

| Tratamiento | Ejemplos de variables |
| --- | --- |
| Heterogéneo dentro del grupo | Skills, formación, background |
| Homogéneo dentro del grupo | Compromiso, responsabilidad, disponibilidad horaria |
| Parejo entre grupos | Nivel promedio, experiencia promedio |

## Lectura para un producto

Todas las herramientas comparten el mismo modelo de cinco piezas:

1. **Entrada:** perfiles por persona (numéricos y categóricos), junto con K grupos o un tamaño por grupo con mínimo y máximo.
2. **Restricciones duras:** tamaños, personas que deben ir juntas o separadas, horarios.
3. **Objetivo blando ponderado:** por variable, decidir si se quiere heterogeneidad, homogeneidad o balance entre grupos.
4. **Solver:** heurística o algoritmo genético para escalar; CP-SAT o ILP si el tamaño lo permite.
5. **Explicabilidad:** mostrar por qué quedó cada grupo así. Casi todas las herramientas son débiles aquí.

**Oportunidad:** lo existente es académico/educativo, open source poco mantenido (gruepr de escritorio, MoodlePeers estancado) o pagado y cerrado (CATME). No apareció una opción moderna, web y genérica (empresas, hackathons, comunidades) con un buen editor de criterios.

## Fuentes

- [Team-Maker: diseño y validación (Georgia Southern)](https://digitalcommons.georgiasouthern.edu/management-facpubs/13)
- [gruepr, an Open Source Tool for Creating Optimal Student Teams (ASEE)](https://peer.asee.org/34717.pdf)
- [gruepr, a Software Tool for Optimally Partitioning Students onto Teams (CoED)](https://coed.asee.org/?p=4162)
- [gruepr en SignPath](https://signpath.org/projects/gruepr)
- [Moodle Group formation plugin](https://moodle.org/plugins/pluginversion.php?id=22303)
- [MoodlePeers: factores relevantes en formación de grupos](https://fitforthem.unipa.it/rep:6533b820fe1ef96bd127a240)
- [anticlust en GitHub](https://github.com/m-Py/anticlust/)
- [Papenberg y Klau (2021), Psychological Methods](https://zendy.io/title/10.1037/met0000301)
- [Teambuilder: integración con OR-Tools](https://github.com/UTDallasEPICS/Teambuilder/wiki/OR%E2%80%90Tools-Integration)
- [TrueSkill (Microsoft Research)](https://www.microsoft.com/en-us/research/project/trueskill/)
- [Ranking and Matchmaking, Graepel y Herbrich](https://microsoft.com/en-us/research/wp-content/uploads/2006/10/Game-Developer-Feature-Article-Graepel-Herbrich.pdf)
- [Matchmaking más allá del skill agregado (arXiv)](https://arxiv.org/pdf/2006.13763v1.pdf)
- [Palubeckis (2011), metaheurísticas para MDGP](https://itc.ktu.lt/index.php/ITC/article/view/977)
- [Gallego et al., tabu search para MDGP](https://fitforthem.unipa.it/rep:6533b836fe1ef96bd12a0ba2)
- [Recocido simulado adaptativo para MDGP (AAAI)](https://ojs.aaai.org/index.php/AAAI/article/view/41006/44967)
- [Lappas et al. (2009), Finding a Team of Experts in Social Networks](https://faculty.cc.gatech.edu/~zha/CSE8801/social-network/p467-lappas.pdf)
- [Maqtary et al. (2019), revisión sistemática CSCL](https://eprints.glos.ac.uk/4867/)
- [Herramienta de IA con roles de Belbin](https://ar5iv.arxiv.org/html/1604.04721)
- [The Composition and Formation of Effective Teams (arXiv)](https://arxiv.org/pdf/1610.08804)
- [Hallazgos sobre composición de equipos (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC10894934/table/tab2)
