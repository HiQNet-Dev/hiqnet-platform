"""Pruebas de la política de tamaño de archivos."""

import unittest

from check_size_policy import HARD_LIMIT, evaluate


class SizePolicyTest(unittest.TestCase):
    def test_archivo_grande_sin_excepcion_falla(self):
        failures, _, _ = evaluate({"a.rs": HARD_LIMIT + 1}, {})
        self.assertEqual(len(failures), 1)
        self.assertIn("límite duro", failures[0])

    def test_excepcion_registrada_permite_el_archivo(self):
        failures, reviews, _ = evaluate({"a.rs": 900}, {"a.rs": {"lines": 900}})
        self.assertEqual(failures, [])
        self.assertEqual(len(reviews), 1)

    def test_crecimiento_sobre_la_linea_base_falla(self):
        failures, _, _ = evaluate({"a.rs": 901}, {"a.rs": {"lines": 900}})
        self.assertEqual(len(failures), 1)
        self.assertIn("línea base", failures[0])

    def test_reduccion_bajo_la_linea_base_es_limpia(self):
        failures, _, _ = evaluate({"a.rs": 700}, {"a.rs": {"lines": 900}})
        self.assertEqual(failures, [])

    def test_archivo_pequeno_no_genera_ruido(self):
        failures, reviews, warnings = evaluate({"a.rs": 120}, {})
        self.assertEqual((failures, reviews, warnings), ([], [], []))


if __name__ == "__main__":
    unittest.main()
