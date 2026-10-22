import { getShot } from '@ejm/shots';
import { ProductShot } from '@ejm/ui/layout/ProductShot';

// Real captures for the layout showcase. The interactive demos keep their own simulated views.
export const Cli = () => <ProductShot shot={getShot('cli', 'tui-read-refused')} alt="ejm-cli reads a file and refuses a write without a grant." frame />;
export const Ide = () => <ProductShot shot={getShot('ide', 'workspace-launch')} alt="The native ejm-ide project tree, source editor and assistant before a model is connected." />;
export const Harness = () => <ProductShot shot={getShot('harness', 'journal')} alt="ejm-harness runs an offline replay session and prints the durable journal events." frame />;
