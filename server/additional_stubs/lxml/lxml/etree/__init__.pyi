from ._classlookup import (
    AttributeBasedElementClassLookup as AttributeBasedElementClassLookup,
    CommentBase as CommentBase,
    CustomElementClassLookup as CustomElementClassLookup,
    ElementBase as ElementBase,
    ElementClassLookup as ElementClassLookup,
    ElementDefaultClassLookup as ElementDefaultClassLookup,
    EntityBase as EntityBase,
    FallbackElementClassLookup as FallbackElementClassLookup,
    ParserBasedElementClassLookup as ParserBasedElementClassLookup,
    PIBase as PIBase,
    PythonElementClassLookup as PythonElementClassLookup,
    set_element_class_lookup as set_element_class_lookup,
)
from ._cleanup import (
    cleanup_namespaces as cleanup_namespaces,
    strip_attributes as strip_attributes,
    strip_elements as strip_elements,
    strip_tags as strip_tags,
)
from ._docloader import Resolver as Resolver
from ._dtd import (
    DTD as DTD,
    DTDError as DTDError,
    DTDParseError as DTDParseError,
    DTDValidateError as DTDValidateError,
)
from ._element import (
    _Attrib as _Attrib,
    _Comment as _Comment,
    _Element as _Element,
    _ElementTree as _ElementTree,
    _Entity as _Entity,
    _ProcessingInstruction as _ProcessingInstruction,
)
from ._factory_func import (
    PI as PI,
    Comment as Comment,
    Element as Element,
    ElementTree as ElementTree,
    Entity as Entity,
    ProcessingInstruction as ProcessingInstruction,
    SubElement as SubElement,
)
from ._iterparse import iterparse as iterparse, iterwalk as iterwalk
from ._module_func import (
    HTML as HTML,
    XML as XML,
    adopt_external_document as adopt_external_document,
    dump as dump,
    fromstring as fromstring,
    fromstringlist as fromstringlist,
    indent as indent,
    iselement as iselement,
    parse as parse,
    register_namespace as register_namespace,
    tostring as tostring,
    tostringlist as tostringlist,
    tounicode as tounicode,
)
from ._module_misc import (
    CDATA as CDATA,
    DEBUG as DEBUG,
    ICONV_COMPILED_VERSION as ICONV_COMPILED_VERSION,
    LIBXML_COMPILED_VERSION as LIBXML_COMPILED_VERSION,
    LIBXML_VERSION as LIBXML_VERSION,
    LXML_VERSION as LXML_VERSION,
    C14NError as C14NError,
    DocInfo as DocInfo,
    DocumentInvalid as DocumentInvalid,
    Error as Error,
    LxmlError as LxmlError,
    LxmlSyntaxError as LxmlSyntaxError,
    QName as QName,
    SchematronError as SchematronError,
    SchematronParseError as SchematronParseError,
    SchematronValidateError as SchematronValidateError,
    __version__ as __version__,
    _Validator as _Validator,
)
from ._nsclasses import (
    ElementNamespaceClassLookup as ElementNamespaceClassLookup,
    FunctionNamespace as FunctionNamespace,
    LxmlRegistryError as LxmlRegistryError,
    NamespaceRegistryError as NamespaceRegistryError,
)
from ._parser import (
    ETCompatXMLParser as ETCompatXMLParser,
    HTMLParser as HTMLParser,
    HTMLPullParser as HTMLPullParser,
    ParseError as ParseError,
    ParserError as ParserError,
    XMLParser as XMLParser,
    XMLPullParser as XMLPullParser,
    XMLSyntaxError as XMLSyntaxError,
    _FeedParser as _FeedParser,
    get_default_parser as get_default_parser,
    set_default_parser as set_default_parser,
)
from ._relaxng import (
    RelaxNG as RelaxNG,
    RelaxNGError as RelaxNGError,
    RelaxNGParseError as RelaxNGParseError,
    RelaxNGValidateError as RelaxNGValidateError,
)
from ._saxparser import (
    ParserTarget as ParserTarget,
    TreeBuilder as TreeBuilder,
    XMLSyntaxAssertionError as XMLSyntaxAssertionError,
)
from ._serializer import (
    C14NWriterTarget as C14NWriterTarget,
    SerialisationError as SerialisationError,
    canonicalize as canonicalize,
    htmlfile as htmlfile,
    xmlfile as xmlfile,
)
from ._xinclude import XInclude as XInclude, XIncludeError as XIncludeError
from ._xmlerror import (
    ErrorDomains as ErrorDomains,
    ErrorLevels as ErrorLevels,
    ErrorTypes as ErrorTypes,
    PyErrorLog as PyErrorLog,
    RelaxNGErrorTypes as RelaxNGErrorTypes,
    _BaseErrorLog as _BaseErrorLog,
    _ErrorLog as _ErrorLog,
    _ListErrorLog as _ListErrorLog,
    _LogEntry as _LogEntry,
    _RotatingErrorLog as _RotatingErrorLog,
    clear_error_log as clear_error_log,
    use_global_python_log as use_global_python_log,
)
from ._xmlid import (
    XMLDTDID as XMLDTDID,
    XMLID as XMLID,
    _IDDict as _IDDict,
    parseid as parseid,
)
from ._xmlschema import (
    XMLSchema as XMLSchema,
    XMLSchemaError as XMLSchemaError,
    XMLSchemaParseError as XMLSchemaParseError,
    XMLSchemaValidateError as XMLSchemaValidateError,
)

# Includes extensions.pxi (XPath func extensions)
from ._xpath import (
    ETXPath as ETXPath,
    Extension as Extension,
    XPath as XPath,
    XPathDocumentEvaluator as XPathDocumentEvaluator,
    XPathElementEvaluator as XPathElementEvaluator,
    XPathError as XPathError,
    XPathEvalError as XPathEvalError,
    XPathEvaluator as XPathEvaluator,
    XPathFunctionError as XPathFunctionError,
    XPathResultError as XPathResultError,
    XPathSyntaxError as XPathSyntaxError,
    _ElementUnicodeResult as _ElementUnicodeResult,
    _XPathEvaluatorBase as _XPathEvaluatorBase,
)
from ._xslt import (
    LIBXSLT_COMPILED_VERSION as LIBXSLT_COMPILED_VERSION,
    LIBXSLT_VERSION as LIBXSLT_VERSION,
    XSLT as XSLT,
    XSLTAccessControl as XSLTAccessControl,
    XSLTApplyError as XSLTApplyError,
    XSLTError as XSLTError,
    XSLTExtension as XSLTExtension,
    XSLTExtensionError as XSLTExtensionError,
    XSLTParseError as XSLTParseError,
    XSLTSaveError as XSLTSaveError,
    _XSLTProcessingInstruction as _XSLTProcessingInstruction,
    _XSLTResultTree as _XSLTResultTree,
)
