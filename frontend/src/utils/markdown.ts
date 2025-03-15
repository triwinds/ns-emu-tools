import DOMPurify from "isomorphic-dompurify";
import { marked } from 'marked';

export default {
  parse(markdown: string) {
    return DOMPurify.sanitize(marked.parse(markdown) as string);
  },
}

