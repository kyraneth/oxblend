"""Owned JSON converter/Record regression contracts (installed wheel)."""
from collections.abc import Mapping
import unittest
import oxblend


class StringMapping(str, Mapping):
    def __iter__(self):
        return iter(('saved',))
    def __len__(self):
        return 1
    def __getitem__(self, key):
        if key != 'saved':
            raise KeyError(key)
        return 7


class CustomRecord(oxblend.Record):
    def items(self):
        return {'custom': 9}.items()


class ConverterContract(unittest.TestCase):
    def test_public_copy_and_nested_projection(self):
        source = {'array': [{'future': [1, True, None, 'native']}],
                  'nested': {'operation_diagnostics': {'kept': 3}}}
        report = oxblend.Record(source)
        source['array'][0]['future'][0] = 99
        expected = {'array': [{'future': [1, True, None, 'native']}],
                    'nested': {'operation_diagnostics': {'kept': 3}}}
        self.assertEqual(report.to_json(), expected)
        changed = report.to_json()
        changed['array'][0]['future'][0] = 88
        self.assertEqual(report.to_json(), expected)
        with self.assertRaises(TypeError):
            report['array'][0]['future'][0] = 2

    def test_scalar_mapping_subclass_keeps_container_semantics(self):
        self.assertEqual(oxblend.Record({'value': StringMapping('text')}).to_json(),
                         {'value': {'saved': 7}})

    def test_record_subclass_keeps_its_mapping_view(self):
        nested = CustomRecord({'original': 1})
        self.assertEqual(oxblend.Record({'nested': nested}).to_json(),
                         {'nested': {'custom': 9}})

    def test_private_owned_freeze_has_the_same_public_shape(self):
        native = {'future': ({'bytes_hex': '00ff',
                              'operation_diagnostics': {'kept': True}},),
                  'number': 2.5, 'empty': (), 'null': None}
        expected = oxblend.Record(native).to_json()
        result = oxblend.Record._from_native_owned(native)
        self.assertIs(type(result), oxblend.Record)
        self.assertIsInstance(result['future'], tuple)
        self.assertIs(type(result['future'][0]), oxblend.Record)
        self.assertEqual(result.to_json(), expected)
        with self.assertRaises(TypeError):
            result['future'][0]['bytes_hex'] = 'changed'

    def test_owned_native_catalog_preserves_all_json_leaves(self):
        native = oxblend._native.error_catalog()
        projected = oxblend.Record(native).to_json()
        self.assertTrue(projected['entries'])
        for before, after in zip(native['entries'], projected['entries'], strict=True):
            self.assertEqual(before['kind'], after['kind'])
            self.assertEqual(before['explanation'], after['explanation'])
            self.assertEqual(list(before['likely_causes']), after['likely_causes'])
            self.assertEqual(list(before['next_steps']), after['next_steps'])
            self.assertEqual('known_issue' in before, 'known_issue' in after)
            if 'known_issue' in before:
                self.assertEqual(before['known_issue'], after['known_issue'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
