# Copias de seguridad y recuperación (SafeEat 2.0)

## Qué hay

| Pieza                                                 | Qué hace                                                                                                                                                                                                                          | Dónde                           |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `scripts/backup-postgres.sh`                          | `pg_dump -Fc` de producción, comprueba que el archivo no está vacío y que su índice se lee (`pg_restore --list`), retiene 14 días                                                                                                 | `/opt/safeeat/backups/postgres` |
| `scripts/verify-backup-restore.sh <dump>`             | **Restaura de verdad** la copia en un contenedor PostGIS desechable (sin puertos publicados, red interna propia, 1 CPU / 1 GB, se borra al terminar) y comprueba migraciones, tablas principales, restricciones y filas huérfanas | local al servidor               |
| `SAFEEAT_VERIFY_RESTORE=1 scripts/backup-postgres.sh` | Copia + verificación por restauración en un solo paso                                                                                                                                                                             |                                 |

`pg_restore --list` solo demuestra que el índice del archivo se lee, no que la copia se pueda restaurar. La verificación por restauración es la prueba real y **no toca producción ni preproducción**.

## Estado

- `verify-backup-restore.sh`: escrito y comprobado sintácticamente (`bash -n`, también en CI). **No se ha ejecutado contra una copia real**: esta auditoría no crea contenedores en el servidor compartido sin autorización. Para validarlo:
  ```bash
  ssh chatink-dev
  cd /opt/safeeat/current   # o la carpeta del despliegue
  bash scripts/verify-backup-restore.sh "$(ls -t /opt/safeeat/backups/postgres/safeeat-*.dump | head -1)"
  ```
  Resultado esperado: `Restore: OK`, `Migrations: OK`, recuento de filas e `Integrity: OK`.
- Programarlo semanalmente: un timer systemd `safeeat-backup-verify.timer` que ejecute el comando anterior (pendiente de autorización, porque cambia la configuración del servidor).

## Copia externa cifrada (preparada, no activada)

Las copias están en el mismo disco que la base de datos: si el disco falla, se pierden las dos. Estrategia recomendada, sin servicios de pago obligatorios:

1. Cifrar con [age](https://age-encryption.org) usando una clave pública. La clave privada se guarda **fuera del servidor**, en un gestor de contraseñas y en papel:
   ```bash
   age -r "$SAFEEAT_BACKUP_AGE_RECIPIENT" -o "$file.age" "$file"
   ```
2. Copiar el `.age` a un destino externo (otro equipo de la red por `rsync` o un almacenamiento de objetos) con una credencial que solo pueda escribir.
3. Retención externa: 7 diarias, 4 semanales y 6 mensuales.
4. Una vez al trimestre: descargar una copia externa, descifrarla en otro equipo y pasar `verify-backup-restore.sh`.

Activarlo requiere una clave y un destino que solo puede elegir el propietario. No se ha inventado ninguno.

## Recuperación (producción)

1. Parar la API (`docker compose stop api web`) para que nadie escriba durante la restauración. Postgres sigue arrancado.
2. Verificar antes la copia elegida con `verify-backup-restore.sh`.
3. Hacer una copia de la base actual aunque esté dañada (`scripts/backup-postgres.sh`) para no perder nada.
4. Restaurar en una base **nueva** (`safeeat_restored`), comprobarla y, solo entonces, cambiar `DATABASE_URL` o renombrar las bases. Nunca restaurar encima de la base en uso.
5. Arrancar la imagen de la versión que generó la copia, o una posterior: las migraciones son solo aditivas.
6. Comprobar `/api/v1/meta`, un inicio de sesión y un producto.

## Despliegues y vuelta atrás

`scripts/deploy-remote.sh` hace una copia antes de desplegar en producción y vuelve a la etiqueta anterior si `/api/v1/meta` falla. Las migraciones son aditivas, así que la imagen anterior funciona con el esquema nuevo. Las migraciones 015 (identidad de evidencia de los veredictos) y 016 (cobertura del catálogo de restaurantes) se han probado en PRE dentro de una transacción revertida (`BEGIN … ROLLBACK`).
