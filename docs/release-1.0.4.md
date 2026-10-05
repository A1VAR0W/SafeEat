# SafeEat 1.0.4

Los motivos de las revisiones se muestran en las fichas de productos y restaurantes. Otra persona puede confirmar que ha comprobado ese motivo; aparece «Verificado manual» en verde con la fecha. La confirmación identifica la revisión, no garantiza la seguridad de un alimento ni sustituye la comprobación en cada local.

Las revisiones positivas de restaurantes se comparten entre locales con la misma identidad de cadena (`brand:wikidata`). No se agrupan por nombres parecidos. La ficha indica cuándo el motivo procede de otro local. Una contradicción local conserva el estado no verificado. Los establecimientos sin identidad de cadena continúan teniendo revisiones locales.

Una persona no puede confirmar su propia revisión. Los reintentos no duplican confirmaciones. Editar una revisión invalida sus confirmaciones y rechaza las solicitudes que aún contienen la revisión anterior. No se exponen identificadores personales ni correos al consultar los motivos.

La migración 006 es aditiva. Después del despliegue hay que reimportar el catálogo para incorporar las identidades de cadena y extender las revisiones positivas existentes. El importador realiza esta operación en una transacción.

Validación: pruebas de API con PostgreSQL/PostGIS aislado para autoría, autenticación, idempotencia, revisiones obsoletas, productos, cadenas, conflictos locales y filtros del catálogo; comprobación de tipos y lint; 140 pruebas unitarias.
