# agrupa

CLI en Rust que forma grupos balanceados a partir de perfiles de alumnos (por ejemplo, las respuestas de un Google Forms) y sugiere a qué grupo sumar a un alumno nuevo sin mover a nadie.

```
alumnos.csv + config.toml ──► agrupa armar   ──► grupos.csv (+ reporte de calidad)
alumnos nuevos            ──► agrupa sugerir ──► grupo recomendado + por qué
```

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
| `--iteraciones`, `--reinicios`, `--semilla` | Esfuerzo de búsqueda; con la misma semilla, el resultado es el mismo |

### Sugerir grupo para alumnos nuevos

```bash
agrupa sugerir -a ejemplos/alumnos.csv -n ejemplos/nuevos.csv -c ejemplos/config.toml -g grupos.csv -s grupos_v2.csv
```

- `-g` es la asignación actual (la que generó `armar`).
- `-n` es opcional. Todo alumno de `-a` que no esté en la asignación también cuenta como nuevo, así que puedes volver a exportar el formulario completo y correr `sugerir` sin separar a los nuevos.
- `-s` guarda la asignación actualizada. Sin esta opción, solo muestra la sugerencia.

### Evaluar una asignación (por ejemplo, una hecha a mano)

```bash
agrupa evaluar -a ejemplos/alumnos.csv -c ejemplos/config.toml -g grupos.csv
```

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
- `[solver]` (opcional): `iterations`, `restarts`, `seed`.

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

```
Calidad (100 % = ideal · 0 % = como un reparto al azar)
  promedio     balancear    97 %   promedios de grupo entre 5.32 y 5.45 (global 5.41)
  habilidades  cubrir      100 %   0 ausencias (mínimo posible 0, al azar ≈ 5.5)
```

En `sugerir`, **Δ** es el cambio en el costo total al sumar al alumno: mientras más bajo, mejor. Junto al Δ se muestra qué criterios mejoran o empeoran y por qué (por ejemplo, "aporta Diseño UX, que faltaba").

## 5. Desarrollo

```bash
cargo test            # tests
cargo clippy          # linter
cargo fmt             # formato
```

| Archivo | Responsabilidad |
| --- | --- |
| `src/config.rs` | Lectura y validación del TOML |
| `src/data.rs` | Lectura del CSV |
| `src/model.rs` | Codificación de alumnos y funciones de costo |
| `src/state.rs` | Costo incremental |
| `src/solver.rs` | Recocido simulado |
| `src/report.rs` | Reportes de texto y JSON |
| `src/suggest.rs` | Sugerencia para alumnos nuevos |
| `src/main.rs` | CLI |
