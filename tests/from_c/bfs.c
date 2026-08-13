#include <stdio.h>
#include <stdlib.h>

#define MAX_NODES 1000

/**
 * Example input file format:
 * 
 1 2 3
 2 4 5
 3 -1 6
 4 -1 -1
 5 -1 -1
 6 -1 -1
 */

// Binary Tree Node
typedef struct Node {
    int val;
    struct Node* left;
    struct Node* right;
} Node;

// Simple Queue for BFS
typedef struct Queue {
    Node* data[MAX_NODES];
    int front;
    int rear;
} Queue;

// Queue helper functions
Queue* createQueue() {
    Queue* q = (Queue*)malloc(sizeof(Queue));
    q->front = 0;
    q->rear = 0;
    return q;
}

int isEmpty(Queue* q) {
    return q->front == q->rear;
}

void enqueue(Queue* q, Node* node) {
    if (q->rear < MAX_NODES) {
        q->data[q->rear++] = node;
    }
}

Node* dequeue(Queue* q) {
    if (isEmpty(q)) return NULL;
    return q->data[q->front++];
}

// Create a new tree node
Node* createNode(int val) {
    Node* node = (Node*)malloc(sizeof(Node));
    node->val = val;
    node->left = NULL;
    node->right = NULL;
    return node;
}

// Load tree from file using parent-child triplets
Node* loadTreeFromFile(const char* filename) {
    FILE* file = fopen(filename, "r");
    if (!file) {
        perror("Error opening file");
        return NULL;
    }

    // Lookup array to map node values to pointer references
    Node* nodeMap[MAX_NODES] = {NULL};
    Node* root = NULL;
    int parentVal, leftVal, rightVal;

    while (fscanf(file, "%d %d %d", &parentVal, &leftVal, &rightVal) == 3) {
        // Retrieve or instantiate parent node
        if (nodeMap[parentVal] == NULL) {
            nodeMap[parentVal] = createNode(parentVal);
        }
        Node* parent = nodeMap[parentVal];

        // The first node processed becomes the root
        if (root == NULL) {
            root = parent;
        }

        // Connect left child if present
        if (leftVal != -1) {
            if (nodeMap[leftVal] == NULL) {
                nodeMap[leftVal] = createNode(leftVal);
            }
            parent->left = nodeMap[leftVal];
        }

        // Connect right child if present
        if (rightVal != -1) {
            if (nodeMap[rightVal] == NULL) {
                nodeMap[rightVal] = createNode(rightVal);
            }
            parent->right = nodeMap[rightVal];
        }
    }

    fclose(file);
    return root;
}

// Breadth-First Search Traversal
void bfs(Node* root) {
    if (!root) return;

    Queue* q = createQueue();
    enqueue(q, root);

    printf("BFS Traversal: ");
    while (!isEmpty(q)) {
        Node* curr = dequeue(q);
        printf("%d ", curr->val);

        if (curr->left)  enqueue(q, curr->left);
        if (curr->right) enqueue(q, curr->right);
    }
    printf("\n");

    free(q);
}

// Free tree memory
void freeTree(Node* root) {
    if (!root) return;
    freeTree(root->left);
    freeTree(root->right);
    free(root);
}

int main() {
    const char* filename = "tree.txt";
    Node* root = loadTreeFromFile(filename);

    if (root) {
        bfs(root);
        freeTree(root);
    } else {
        printf("Failed to load tree from %s.\n", filename);
    }

    return 0;
}