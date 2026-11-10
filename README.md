# agrupa

CLI en Rust que forma grupos balanceados a partir de perfiles de alumnos (por ejemplo, las respuestas de un Google Forms) y sugiere a qué grupo sumar a un alumno nuevo sin mover a nadie.

```
alumnos.csv + config.toml ──► agrupa armar   ──► grupos.csv (+ reporte de calidad)
alumnos nuevos            ──► agrupa sugerir ──► grupo recomendado + por qué
```

Cómo funciona por dentro (funciones de costo, normalización y algoritmo): [Modelo.md](Modelo.md).

## 1. Instalación en un PC nuevo (Windows + WSL)

### 1.1 WSL con Ubuntu (solo la primera vez)

En PowerShell como administrador:

```powershell
wsl --install -d Ubuntu
```

Reinicia y abre la app "Ubuntu". Todos los comandos que siguen se corren ahí.

### 1.2 Rust y herramientas

```bash
sudo apt update && sudo apt install -y build-essential curl pkg-config git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # elige la opción 1 (default)
source "$HOME/.cargo/env"
rustc --version   # debe ser 1.88 o superior (si no: rustup update)
```

### 1.3 Obtener el proyecto y compilar

Trabaja dentro de Linux (`~/...`), no en `/mnt/c/...`: compilar desde el disco de Windows es mucho más lento.

```bash
mkdir -p ~/proyectos && cd ~/proyectos
git clone https://github.com/<tu-usuario>/agrupa.git
cd agrupa
cargo build --release
cargo test
```

El ejecutable queda en `target/release/agrupa`.

Opcional: instalarlo como comando global, para usar `agrupa` desde cualquier carpeta:

```bash
cargo install --path .
```

### 1.4 Editor (opcional)

En VS Code, instala las extensiones **WSL** y **rust-analyzer**. Luego, desde la carpeta del proyecto, corre `code .`.

## 2. Uso

Hay dos formas de ejecutar: con `cargo run --release -- <comando> ...` (compila si hace falta) o directamente con `./target/release/agrupa <comando> ...`. Los ejemplos usan `agrupa`.

### Armar grupos desde cero

```bash
agrupa armar -a ejemplos/alumnos.csv -c ejemplos/config.toml -s grupos.csv
```

| Opción | Descripción |
| --- | --- |
| `-a, --alumnos` | CSV con los alumnos |
| `-c, --config` | Criterios (TOML) |
| `-s, --salida` | CSV de salida (por defecto `grupos.csv`) |
| `--json` | Guarda además el detalle en JSON |
| `--iteraciones`, `--reinicios`, `--semilla` | Esfuerzo de búsqueda (por defecto 1.000.000, 8 y 42, o lo que diga `[solver]`). Con la misma semilla, el resultado es el mismo |

### Sugerir grupo para alumnos nuevos

```bash
agrupa sugerir -a ejemplos/alumnos.csv -n ejemplos/nuevos.csv -c ejemplos/config.toml -g grupos.csv -s grupos_v2.csv
```

- `-g` es la asignación actual (la que generó `armar`).
- `-n` es opcional. Todo alumno de `-a` que no esté en la asignación también cuenta como nuevo, así que puedes volver a exportar el formulario completo y correr `sugerir` sin separar a los nuevos.
- `-s` guarda la asignación actualizada. Sin esta opción, solo muestra la sugerencia.
- Si llegan varios nuevos a la vez, se reparten entre ellos para el mejor resultado conjunto. A los que ya tenían grupo no se los mueve.
- Si `config.toml` no define `max_size`, primero se llenan los grupos más chicos.

### Evaluar una asignación (por ejemplo, una hecha a mano)

```bash
agrupa evaluar -a ejemplos/alumnos.csv -c ejemplos/config.toml -g grupos.csv
agrupa evaluar -a ejemplos/alumnos.csv -n ejemplos/nuevos.csv -c ejemplos/config.toml -g grupos_v2.csv
```

`-n` es opcional y se suma a `-a`, igual que en `sugerir`. `--json` guarda el detalle.

### Ayuda

```bash
agrupa --help
agrupa armar --help
```

## 3. Formato de datos

### 3.1 Alumnos (CSV de entrada)

Una fila por alumno y la primera fila con encabezados. Sirve tal cual el CSV que exporta Google Forms o Excel.

```csv
id,nombre,genero,carrera,promedio,compromiso,horario,habilidades,quiero_trabajar_con,prefiero_evitar
a01,Antonia Carrasco,F,Ing. Computación,"5,6",2,Tarde,"Frontend, Backend",a06,
a02,Benjamín Reyes,M,Ing. Civil Industrial,"5,3",3,Mañana,Backend,,
a03,Catalina Muñoz,F,Ing. Civil Industrial,"5,0",5,Mañana,"Backend, Datos, Gestión",a10,a14
```

| Regla | Detalle |
| --- | --- |
| Separador | `,` `;` o tab, detectado automáticamente (Excel en español suele usar `;`) |
| Codificación | UTF-8 (con o sin BOM) |
| Columna de id | Obligatoria y única. Si un id se repite, se usa la **última** fila (útil cuando alguien responde dos veces) |
| Números | `5.8` o `5,8` |
| Valores múltiples | En una celda, separados por coma o punto y coma: `"Backend, Datos"` (formato de casillas de Google Forms) |
| Preferencias | Ids o nombres exactos de compañeros, separados por coma |
| Celdas vacías | Se permiten. Un número vacío cuenta como el promedio; una categoría vacía no cuenta. El programa lo avisa |
| Otras columnas | Se ignoran (email, marca temporal, etc.) |

Los nombres de las columnas los defines tú en `config.toml`.

### 3.2 Criterios (`config.toml`)

```toml
id_column = "id"
name_column = "nombre"

[groups]
size = 4            # alumnos por grupo (o `count = 8` para fijar cantidad de grupos)
# min_size = 3
# max_size = 5

[[attributes]]
column = "promedio"
kind = "numeric"
goal = "balance"
weight = 2.0

[[attributes]]
column = "habilidades"
kind = "tags"
goal = "cover"

[preferences]
prefer_column = "quiero_trabajar_con"
avoid_column = "prefiero_evitar"

[constraints]
apart = [["a02", "a13"]]
```

| `kind` | `goal` | Qué busca | Ejemplo |
| --- | --- | --- | --- |
| `numeric` | `balance` | Promedios parejos entre grupos | Notas, experiencia |
| `numeric` | `similar` | Valores parecidos dentro del grupo | Compromiso, horas disponibles |
| `categorical` | `spread` | Misma proporción en todos los grupos | Género |
| `categorical` | `together` | Mismo valor dentro del grupo | Horario, sección |
| `categorical` | `cover` | La mayor variedad posible por grupo | Carrera |
| `tags` | `spread` | Cada etiqueta repartida en proporción | Intereses |
| `tags` | `cover` | Que cada grupo tenga todas las etiquetas | Habilidades, roles |

- `weight`: importancia relativa (por defecto 1). Con 1, pasar de un reparto al azar a uno perfecto en ese criterio vale 1, así que los pesos se comparan entre columnas de distinta escala.
- `[preferences]`: blandas, se intentan cumplir.
- `[constraints]`: duras, `together` (deben ir juntos) y `apart` (deben ir separados).
- `[solver]` (opcional): `iterations` (por defecto 1.000.000 por reinicio), `restarts` (8, en paralelo) y `seed` (42).

En `ejemplos/config.toml` hay un ejemplo completo y comentado.

### 3.3 Asignación (CSV de salida)

```csv
id,nombre,grupo
a03,Catalina Muñoz,1
a07,Gaspar Pizarro,1
...
```

Es la entrada `-g` de `sugerir` y `evaluar`. Puedes editarla a mano: solo se leen las columnas `id` y `grupo`.

## 4. Cómo leer el reporte

### `armar` y `evaluar`

```
8 grupos: 6 de 4 y 2 de 3 alumnos

Calidad (100 % = ideal · 0 % = como un reparto al azar)
  promedio      balancear    96 %   promedios de grupo entre 5.30 y 5.50 (global 5.41)
  compromiso    homogéneo    54 %   dispersión dentro de los grupos 0.74 (mínimo ≈ 0.29, al azar ≈ 1.05)
  género        repartir    100 %   F 1–2 · M 1–2 por grupo
  habilidades   cubrir      100 %   1 ausencia (mínimo posible 1, al azar ≈ 7.6)
  preferencias  cumplir      38 %   6 de 14 elecciones cumplidas · 8 sin ninguno de sus elegidos · ...
Restricciones: todas cumplidas (1 juntos · 1 separados)

Grupo 1 (4): Antonia Carrasco, Josefa Vergara, Matías Espinoza, Joaquín Valenzuela
   promedio 5.30 · compromiso 2–4 · género F ×2, M ×2
   carrera Ing. Computación, Ing. Civil Industrial, Ing. Eléctrica, Ing. Matemática
   horario Tarde ×2, Mañana ×2 · habilidades: todas
```

- Cada criterio se mide de 0 % (como un reparto al azar) a 100 % (lo mejor posible solo para ese criterio). Puede ser negativo si una asignación hecha a mano queda peor que el azar.
- Rara vez se llega a 100 % en todos: los criterios compiten entre sí. Por ejemplo, juntar por horario choca con mezclar carreras. Si un criterio importa más, súbele el `weight`.
- Ojo con los criterios correlacionados. Si el compromiso va de la mano con las notas, pedir compromiso `similar` es pedir grupos de buenas notas, lo contrario de `balance` en promedio.
- Bajo los criterios viene cada grupo con su resumen, para ver por qué quedó así.

### `sugerir`

```
Paula Henríquez (a31) → grupo 3
     grupo  tamaño       Δ
   ★ 3       3 → 4    -0.181
     agrandando un grupo: 4 (4 → 5, -0.108) · 1 (4 → 5, +0.027) · 2 (4 → 5, +0.056)
   Por qué el grupo 3:
     mejora   habilidades    -0.295  aporta Diseño UX, que faltaba
     mejora   carrera        -0.178  aporta Diseño, que no estaba
     empeora  género         +0.138  F: 2 → 3 (proporcional 2)
     empeora  preferencias   +0.072  eligió a Sofía Navarro, que está en el grupo 6
```

- **Δ** es el cambio en el costo total al sumar al alumno: mientras más bajo, mejor. Usa la misma escala que los pesos: con `weight = 1`, pasar de "al azar" a "ideal" en un criterio vale 1.
- Arriba van los grupos con cupo. "agrandando un grupo" muestra las mejores opciones que dejarían un grupo más grande que el resto, por si prefieres aceptar esa diferencia.
- "Por qué" desglosa el Δ por criterio, de mayor a menor efecto.

## 5. Desarrollo

```bash
cargo test            # tests
cargo clippy          # linter
cargo fmt             # formato
```

| Archivo | Responsabilidad |
| --- | --- |
| `src/main.rs`, `src/commands.rs` | CLI: argumentos y los comandos `armar`, `sugerir`, `evaluar` |
| `src/config.rs`, `src/data.rs` | Lectura del TOML y del CSV |
| `src/roster.rs` | Alumnos: ids, nombres y búsqueda |
| `src/relations.rs` | Preferencias y restricciones `together`/`apart` |
| `src/criteria/` | Un archivo por criterio (`balance.rs`, `spread.rs`, `cover.rs`...): costo, referencias y explicaciones |
| `src/objective.rs` | Qué se minimiza: criterios con su escala |
| `src/partition.rs` | Quién está en qué grupo |
| `src/state.rs` | Partición + costo al día tras cada cambio |
| `src/problem.rs` | Junta alumnos, relaciones y criterios |
| `src/solver/` | Búsqueda: tamaños, reparto inicial, movimientos, recocido y pulido |
| `src/suggest.rs` | Sugerencia para alumnos nuevos |
| `src/report.rs`, `src/assignment.rs` | Reporte de calidad y CSV de asignación |
| `tests/ejemplos.rs` | Pruebas de punta a punta con `ejemplos/` |

Para agregar un criterio nuevo: un archivo en `src/criteria/` que implemente `GroupwiseCriterion` (o `Criterion`, si no se suma por grupo) y una línea en `from_attribute` de `src/criteria/mod.rs`.
