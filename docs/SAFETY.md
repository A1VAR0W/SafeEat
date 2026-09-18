# Evidence model and audit boundaries

`CONTAINS_GLUTEN` takes precedence over `MAY_CONTAIN_TRACES`, then a non-contradictory explicit declaration, then `UNKNOWN`. Oats without a gluten-free declaration remain unknown. The input provenance and literal matched evidence are returned alongside reasons and engine version. Confidence is evidence confidence, not a probability that eating the product is safe.

Spanish and English are the supported languages. Unicode compatibility normalization, accent folding, case, punctuation and phrase context are explicit. The engine is a conservative rules system, not a medical classifier. It cannot detect a missing OCR line, verify a manufacturer's certification, determine a laboratory ppm measurement or validate community data. Reviewed OCR claims retain limited confidence. Never replace label inspection with this result.

Community gluten reports are stored separately from the rules-engine assessment. A single user's statement never changes the underlying assessment or certifies a product. Conflicting reports are shown as conflicting, and a reported risk can suppress a favorable display label. Restaurant dietary tags and user reports describe options but do not verify preparation or cross-contact controls.

SafeEat Quality Score v1 is a transparent, unvalidated heuristic, independent of gluten. Six required numeric inputs per 100 g/ml: energy, sugar, saturated fat, salt, fibre and protein. Invalid, absent or implausibly inconsistent values produce no score. The score is rounded to multiples of five to avoid false precision. Nutri-Score from OFF is shown separately.

Regression tests cover positive and negated claims, contradictory evidence, traces, oats, Unicode, invalid nutrients and provider normalization. Security integration tests cover hashed credentials, user isolation, rotating-token replay, logout, account deletion and public data minimization. Physical-device OCR/camera, nutritional validation and manufacturer verification remain distinct from software tests.
