# ReAmped

ReAmped es un reproductor de música escrito en Rust con una interfaz gráfica en `egui` y backend de audio basado en `symphonia`.

## Resumen del refactor

Este refactor se centró en volver más robusto el flujo de carga y selección de temas:

- El escaneo de archivos ahora es recursivo y tolerante a archivos no válidos o no compatibles.
- La reproducción inicial se volvió atómica para evitar estados intermedios donde el tema quedaba seleccionado pero aún no cargado en el backend.
- La navegación de la playlist dejó de depender solo de índices y ahora usa la ruta del archivo como identidad estable.
- La UI mantiene el tema actual seleccionado incluso cuando está en pausa.
- La sincronización de metadatos y estado visual se hizo más consistente al cambiar de tema o reordenar la playlist.

## Nuevas funciones agregadas

- Soporte para lanzar la app con archivos o carpetas como argumentos de línea de comandos.
- Reproducción automática al iniciar si se pasaron temas por CLI.
- Integración con controles multimedia en Linux mediante MPRIS.
- Reordenamiento de playlist sin romper la selección del tema actual.
- Búsqueda y selección en la mini playlist usando rutas estables del archivo.
- Manejo más seguro de metadatos y carátulas cuando faltan datos o el archivo está dañado.

## Comportamiento importante

- Si pausas un tema, seguirá viéndose como seleccionado.
- Si reordenas o mezclas la playlist, el tema actual se conserva por ruta, no por índice.
- Si un archivo no es de audio o está corrupto, el escaneo lo ignora en lugar de fallar.

## Estructura

- `desktop/`: aplicación de escritorio y UI.
- `player-core/`: lógica de reproducción, estado y backend de audio.
- `assets/`: recursos embebidos como fuentes e imágenes.

## Ejecución

Desde la carpeta `desktop/`:

```bash
cargo run
```

Si quieres probar el arranque con archivos o carpetas:

```bash
cargo run -- /ruta/a/tu/musica
```
